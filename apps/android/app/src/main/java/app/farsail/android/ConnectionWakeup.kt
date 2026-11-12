package app.farsail.android

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager
import android.os.SystemClock
import java.util.Timer
import java.util.TimerTask
import java.util.concurrent.atomic.AtomicBoolean

/** Real CPU wake delivery for an explicitly enabled, authenticated foreground task. */
internal class ConnectionWakeup(private val context: Context) : AutoCloseable {
    companion object {
        private fun operation(context: Context) = PendingIntent.getBroadcast(context, 43,
            Intent(context, ConnectionWakeReceiver::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        fun cancel(context: Context) { context.getSystemService(AlarmManager::class.java).cancel(operation(context)) }
    }
    private val manager = context.getSystemService(AlarmManager::class.java)
    private val plan = ConnectionWakePlan({ deadline ->
        if (Build.VERSION.SDK_INT >= 31 && !manager.canScheduleExactAlarms()) throw SecurityException("Sleep wake scheduling unavailable")
        manager.setExactAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, deadline, operation(context))
    }, { cancel(context) })
    var deliveries = 0L
        private set
    var lastDelivery = 0L
        private set
    private var unavailable = false
    fun eligible() = BackgroundSetup.exempt(context) &&
        !context.getSystemService(PowerManager::class.java).isInteractive &&
        !context.getSystemService(BatteryManager::class.java).isCharging &&
        (Build.VERSION.SDK_INT < 31 || manager.canScheduleExactAlarms())
    fun ensure(active: Boolean) {
        try { plan.ensure(SystemClock.elapsedRealtime(), active && eligible()); unavailable = false }
        catch (_: SecurityException) { unavailable = true }
    }
    fun delivered(): Boolean {
        if (!plan.delivered()) return false
        deliveries++; lastDelivery = SystemClock.elapsedRealtime()
        return true
    }
    fun diagnostic() = "wakeAlarmAtMs=${plan.nextAt} wakeAlarmDeliveries=$deliveries wakeAlarmAgeMs=${if (lastDelivery == 0L) -1 else SystemClock.elapsedRealtime() - lastDelivery} wakeAlarmUnavailable=$unavailable"
    override fun close() { plan.close() }
}

/** Not exported: an alarm can only request a fresh heartbeat under the live scope. */
class ConnectionWakeReceiver : BroadcastReceiver() {
    companion object { private val deadlines = Timer("farsail-wake-delivery", true) }
    override fun onReceive(context: Context, intent: Intent) {
        val service = ConnectionService.instance
        if (service == null) { ConnectionWakeup.cancel(context); return }
        val pending = goAsync()
        val finished = AtomicBoolean(false)
        val timeout = object : TimerTask() {
            override fun run() { if (finished.compareAndSet(false, true)) pending.finish() }
        }
        // AlarmManager keeps the CPU available for delivery; cap asynchronous
        // delivery independently of the UI looper and any stalled network call.
        deadlines.schedule(timeout, 8000)
        service.wakeHeartbeat {
            if (finished.compareAndSet(false, true)) { timeout.cancel(); pending.finish() }
        }
    }
}
