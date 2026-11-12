package app.farsail.android

import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.os.PowerManager
import android.os.Handler
import android.os.Looper
import android.os.SystemClock

/** Scoped to the explicit projection service, including when its Activity is in the background. */
class SharingPower(private val context: Context) : AutoCloseable {
    companion object {
        fun enabled(context: Context) = context.getSharedPreferences("sharing", Context.MODE_PRIVATE).getBoolean("keepScreenOn", true)
        fun preference(context: Context, value: Boolean) {
            context.getSharedPreferences("sharing", Context.MODE_PRIVATE).edit().putBoolean("keepScreenOn", value).apply()
            if (CaptureService.running) context.startService(Intent(context, CaptureService::class.java).setAction(CaptureService.POWER))
        }
    }
    private val manager = context.getSystemService(PowerManager::class.java)
    private val handler = Handler(Looper.getMainLooper())
    private var running = false
    private var renewedAt = 0L
    private val cpu = manager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "FarSail:Sharing").apply { setReferenceCounted(false) }
    @Suppress("DEPRECATION")
    private val screen = manager.newWakeLock(PowerManager.SCREEN_DIM_WAKE_LOCK, "FarSail:SharingScreen").apply { setReferenceCounted(false) }
    private fun lease(lock: PowerManager.WakeLock) = object : SharingLease.Lock {
        override val held get() = lock.isHeld
        override fun acquire(timeoutMs: Long) { lock.acquire(timeoutMs) }
        override fun release() { lock.release() }
    }
    private val leases = SharingLease(lease(cpu), lease(screen))
    private val renew = object : Runnable {
        override fun run() {
            if (!running) return
            keepScreen(enabled(context))
            handler.postDelayed(this, SharingLease.PERIOD)
        }
    }
    fun start() {
        if (running) return
        running = true
        renew.run()
    }
    @Suppress("DEPRECATION")
    fun keepScreen(enabled: Boolean) {
        if (!running) return
        // A foreground service must cover other apps too. FLAG_KEEP_SCREEN_ON
        // only covers our visible Activity. Do not wake or unlock the device.
        leases.refresh(enabled && manager.isInteractive && !context.getSystemService(KeyguardManager::class.java).isKeyguardLocked)
        renewedAt = SystemClock.elapsedRealtime()
    }
    fun diagnostic() = "sharingCpuHeld=${cpu.isHeld} sharingScreenHeld=${screen.isHeld} powerRenewalAgeMs=${if (renewedAt == 0L) -1 else SystemClock.elapsedRealtime() - renewedAt}"
    override fun close() {
        running = false
        handler.removeCallbacks(renew)
        leases.close()
    }
}
