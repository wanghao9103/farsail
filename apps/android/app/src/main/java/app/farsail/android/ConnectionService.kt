package app.farsail.android

import android.app.*
import android.content.*
import android.content.pm.ServiceInfo
import android.os.*
import org.json.JSONObject

/** Device presence and its authenticated endpoint do not grant screen capture. */
class ConnectionService : Service() {
    companion object {
        private const val STOP = "app.farsail.android.STOP_CONNECTION"
        private const val PERIOD = 15000L
        @Volatile var running = false
            private set
        @Volatile var lastHeartbeat = 0L
            private set
        @Volatile var detail = "打开 App 后自动保持本机在线。"
            private set
        private var attemptedScope: String? = null
        private var activeScope: String? = null
        fun enabled(context: Context) = context.getSharedPreferences("connection", Context.MODE_PRIVATE).getBoolean("keepOnline", true)
        fun scope(state: JSONObject): String? {
            val server = state.optString("server").trimEnd('/')
            val device = state.optString("deviceId")
            return if (state.optBoolean("signedIn") && server.isNotBlank() && device.isNotBlank() && device != "null") "$server|$device" else null
        }
        fun preference(context: Context, enabled: Boolean) {
            val edit = context.getSharedPreferences("connection", Context.MODE_PRIVATE).edit().putBoolean("keepOnline", enabled)
            if (!enabled) edit.remove("scope")
            edit.commit()
            attemptedScope = null
            if (!enabled) stop(context)
            (context.applicationContext as FarSailApp).observer?.invoke()
        }
        fun retryOnForeground() { if (!running) attemptedScope = null }
        fun stop(context: Context) {
            attemptedScope = null
            context.getSharedPreferences("connection", Context.MODE_PRIVATE).edit().remove("scope").commit()
            context.stopService(Intent(context, ConnectionService::class.java))
        }
        fun synchronize(context: Context, state: JSONObject, foreground: Boolean) {
            val scope = scope(state)
            if (!enabled(context) || scope == null) { attemptedScope = null; if (running) stop(context); return }
            if (!foreground || (running && activeScope == scope) || attemptedScope == scope) return
            attemptedScope = scope
            context.getSharedPreferences("connection", Context.MODE_PRIVATE).edit().putString("scope", scope).commit()
            try { context.startForegroundService(Intent(context, ConnectionService::class.java).putExtra("scope", scope)) }
            catch (e: Exception) {
                android.util.Log.w("FarSailConnection", "Start request failed: ${e.javaClass.simpleName}")
                detail = "后台在线服务未启动，重新打开 App 可重试；请检查后台运行设置。"
            }
        }
    }
    private val app get() = application as FarSailApp
    private val handler = Handler(Looper.getMainLooper())
    @Volatile private var stopped = false
    private var inflight = false
    @Volatile private var generation = 0L
    private var identity = ""
    private var startedAt = 0L
    private lateinit var cpu: PowerManager.WakeLock
    private var lastNotification = ""
    @Volatile private var operation = "idle"
    private var lastTick = 0L

    override fun onCreate() {
        super.onCreate()
        cpu = getSystemService(PowerManager::class.java).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "FarSail:Connection").apply { setReferenceCounted(false) }
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel("connection", "设备在线", NotificationManager.IMPORTANCE_LOW).apply {
                description = "保持设备连接；屏幕共享仍需单独授权"; setSound(null, null); enableVibration(false); setShowBadge(false)
            })
    }
    private fun notification(): Notification {
        val open = PendingIntent.getActivity(this, 10, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val stop = PendingIntent.getService(this, 11, Intent(this, ConnectionService::class.java).setAction(STOP), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        return Notification.Builder(this, "connection").setSmallIcon(R.drawable.ic_notification)
            .setContentTitle("FarSail 设备连接").setContentText(detail).setContentIntent(open)
            .setOngoing(true).setOnlyAlertOnce(true)
            .addAction(Notification.Action.Builder(null, "停止后台在线", stop).build()).build()
    }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == STOP) { preference(this, false); stopSelf(); return START_NOT_STICKY }
        // System recreation can deliver a null Intent. Only the last deliberate
        // authenticated scope may be restored, and the tick rechecks native auth.
        val next = intent?.getStringExtra("scope") ?: getSharedPreferences("connection", Context.MODE_PRIVATE).getString("scope", null)
        if (next.isNullOrEmpty() || !enabled(this)) { stopSelf(); return START_NOT_STICKY }
        identity = next; generation++; inflight = false; lastHeartbeat = 0; startedAt = SystemClock.elapsedRealtime()
        detail = "正在恢复设备在线状态…"
        try {
            if (Build.VERSION.SDK_INT >= 29) startForeground(42, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
            else startForeground(42, notification())
            running = true; activeScope = identity
            handler.removeCallbacks(tick); handler.post(tick)
        } catch (e: Exception) {
            android.util.Log.w("FarSailConnection", "Foreground start failed: ${e.javaClass.simpleName}")
            detail = "后台在线服务未启动，请检查后台运行设置后重新打开 App。"; stopSelf()
        }
        return START_STICKY
    }
    private val tick = object : Runnable {
        override fun run() {
            if (stopped) return
            lastTick = SystemClock.elapsedRealtime()
            if (!enabled(this@ConnectionService)) { stopSelf(); return }
            if (!app.ready && SystemClock.elapsedRealtime() - startedAt > 45000) { stopSelf(); return }
            // Bounded leases are renewed only while this visible connection task lives.
            // No screen wake lock, screen wake-up or keyguard operation is involved.
            cpu.acquire(PERIOD * 6)
            if (!inflight && app.ready) {
                inflight = true
                operation = "queued"
                val epoch = generation; val expected = identity
                app.io.execute {
                    var current = true
                    var success = false
                    try {
                        operation = "identity"
                        current = !stopped && generation == epoch && enabled(this@ConnectionService) && scope(app.invoke("state") as JSONObject) == expected
                        if (current) { operation = "presence"; app.invoke("presence"); success = true; operation = "refresh"; app.refresh() }
                    } catch (e: Exception) {
                        val code = Regex("HTTP ([0-9]{3})").find(e.message.orEmpty())?.groupValues?.get(1) ?: e.javaClass.simpleName
                        android.util.Log.w("FarSailConnection", "Presence failed: $code")
                        if (Regex("HTTP (401|403)([^0-9]|$)").containsMatchIn(e.message.orEmpty())) current = false
                    }
                    handler.post {
                        if (stopped || generation != epoch) return@post
                        inflight = false
                        operation = "idle"
                        if (!current) { stopSelf(); return@post }
                        if (success) {
                            lastHeartbeat = SystemClock.elapsedRealtime()
                            detail = if (CaptureService.running && CaptureService.phase == "sharing") "设备在线，屏幕正在共享。" else "设备在线；屏幕未共享，查看前仍需系统授权。"
                            android.util.Log.i("FarSailConnection", "Heartbeat accepted; capture=${CaptureService.running}")
                        } else detail = "设备连接暂不可用，正在重试；请检查网络或登录状态。"
                        if (detail != lastNotification) {
                            lastNotification = detail
                            if (Build.VERSION.SDK_INT < 33 || checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) == android.content.pm.PackageManager.PERMISSION_GRANTED)
                                getSystemService(NotificationManager::class.java).notify(42, notification())
                            app.observer?.invoke()
                        }
                    }
                }
            }
            handler.postDelayed(this, PERIOD)
        }
    }
    override fun dump(fd: java.io.FileDescriptor, writer: java.io.PrintWriter, args: Array<out String>) {
        val now = SystemClock.elapsedRealtime()
        writer.println("running=$running ready=${app.ready} inflight=$inflight operation=$operation")
        writer.println("heartbeatAgeMs=${if (lastHeartbeat == 0L) -1 else now - lastHeartbeat} tickAgeMs=${if (lastTick == 0L) -1 else now - lastTick}")
        writer.println("batteryExempt=${BackgroundSetup.exempt(this)} cpuLeaseHeld=${::cpu.isInitialized && cpu.isHeld}")
    }
    override fun onDestroy() {
        stopped = true; generation++; running = false; activeScope = null
        detail = if (enabled(this)) "后台在线服务已停止，重新打开 App 可重试。" else "后台保持在线已关闭。"
        handler.removeCallbacks(tick)
        if (::cpu.isInitialized && cpu.isHeld) cpu.release()
        stopForeground(STOP_FOREGROUND_REMOVE)
        app.observer?.invoke()
        super.onDestroy()
    }
    override fun onBind(intent: Intent?) = null
}
