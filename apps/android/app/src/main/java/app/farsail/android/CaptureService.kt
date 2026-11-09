package app.farsail.android

import android.app.*
import android.content.*
import android.content.pm.ServiceInfo
import android.graphics.PixelFormat
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.ImageReader
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.*
import android.view.Display
import android.view.WindowManager
import org.json.JSONObject
import kotlin.math.roundToInt

class CaptureService : Service() {
    companion object {
        const val POWER = "app.farsail.android.POWER"
        const val STOP = "app.farsail.android.STOP"
        const val ACCESSIBILITY = "app.farsail.android.ACCESSIBILITY_CAPTURE"
        @Volatile var running = false
        @Volatile var accessibilityMode = false
        @Volatile var screenshotFrames = 0L
        @Volatile var phase = "off"
        @Volatile var detail = "电脑暂时无法查看这台手机。"
        fun status(context: Context, next: String, text: String) {
            phase = next; detail = text
            android.util.Log.i("FarSailSharing", "State: $next")
            val app = context.applicationContext as FarSailApp
            app.main.post { app.observer?.invoke() }
        }
        @Volatile var screenWidth = 0
        @Volatile var screenHeight = 0
        fun stop(context: Context) { RemotePreferences.stopSharing(context); status(context, "off", "共享已停止，电脑无法继续查看。"); context.stopService(Intent(context, CaptureService::class.java)) }
        fun accessibilityRevoked(context: Context) {
            if (!accessibilityMode) return
            NativeBridge.stopLocal()
            status(context, "interrupted", "辅助功能服务已断开，截图共享已停止。恢复系统授权后返回 App 即可重试。")
            context.stopService(Intent(context, CaptureService::class.java))
        }
    }
    private val app get() = application as FarSailApp
    private val handler = Handler(Looper.getMainLooper())
    private val worker = HandlerThread("farsail-capture")
    private lateinit var capture: Handler
    private var projection: MediaProjection? = null
    private var display: VirtualDisplay? = null
    private var reader: ImageReader? = null
    private var accessibilityFrames: AccessibilityFrames? = null
    private var announced = false
    private lateinit var power: SharingPower
    private var epoch = -1L
    private var lastHeartbeat = 0L
    private var polling = false
    @Volatile private var stopped = false
    private var layout = 0L
    private var lastFrame = 0L
    // onCapturedContentResize is only delivered on Android 14+. Older phones
    // must resize the capture surface when the physical display rotates too.
    private val displayListener = object : DisplayManager.DisplayListener {
        override fun onDisplayAdded(displayId: Int) { }
        override fun onDisplayRemoved(displayId: Int) { }
        override fun onDisplayChanged(displayId: Int) {
            if (displayId != Display.DEFAULT_DISPLAY || stopped || !running || projection == null) return
            try {
                val (width, height) = dimensions(this@CaptureService)
                resize(width, height)
            } catch (e: Exception) { fail(e) }
        }
    }
    private val callback = object : MediaProjection.Callback() {
        override fun onStop() {
            if (stopped) return
            val locked = getSystemService(KeyguardManager::class.java).isKeyguardLocked || !getSystemService(PowerManager::class.java).isInteractive
            NativeBridge.stopLocal()
            handler.post {
                if (!stopped && phase != "failed") {
                    status(this@CaptureService, "interrupted", if (locked) "手机锁屏，系统已结束录屏授权。解锁后点「恢复屏幕共享」，并确认系统授权；已保存的远程控制开关无需重设。" else "系统已结束录屏授权。点「恢复屏幕共享」重新授权即可，远程控制设置已保留。")
                    stopSelf()
                }
            }
        }
        override fun onCapturedContentResize(width: Int, height: Int) {
            if (!stopped && width > 0 && height > 0) resize(width, height)
        }
    }
    override fun onCreate() {
        super.onCreate()
        worker.start(); capture = Handler(worker.looper)
        power = SharingPower(this)
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("sharing", "屏幕共享", NotificationManager.IMPORTANCE_LOW).apply {
            description = "共享期间的状态与停止入口"; setSound(null, null); enableVibration(false); setShowBadge(false)
        })
    }
    private fun notification(): Notification {
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val stop = PendingIntent.getService(this, 1, Intent(this, CaptureService::class.java).setAction(STOP), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        return Notification.Builder(this, "sharing").setSmallIcon(R.drawable.ic_notification)
            .setContentTitle("FarSail 屏幕共享中")
            .setContentText(if (app.pending.length() > 0) "有连接请求，请打开 FarSail 确认" else "打开管理连接，或停止共享")
            .setContentIntent(open).setOngoing(true).setOnlyAlertOnce(true)
            .addAction(Notification.Action.Builder(null, "停止共享", stop).build()).build()
    }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == STOP) { RemotePreferences.stopSharing(this); status(this, "off", "共享已停止，电脑无法继续查看。"); NativeBridge.stopLocal(); ControlService.cancel(); stopSelf(); return START_NOT_STICKY }
        if (intent?.action == POWER) {
            if (running) power.keepScreen(SharingPower.enabled(this)) else stopSelf()
            return START_NOT_STICKY
        }
        if (running || intent == null) { if (!running) stopSelf(); return START_NOT_STICKY }
        if (intent.action == ACCESSIBILITY) return startAccessibility()
        val data = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra("data", Intent::class.java) else @Suppress("DEPRECATION") intent.getParcelableExtra("data")
        if (data == null || intent.getIntExtra("result", 0) != Activity.RESULT_OK) { stopSelf(); return START_NOT_STICKY }
        status(this, "starting", "已获得系统授权，正在连接服务器…")
        try {
            if (Build.VERSION.SDK_INT >= 29) startForeground(41, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
            else startForeground(41, notification())
            epoch = NativeBridge.begin()
            check(epoch >= 0) { "请等待初始化完成" }
            projection = getSystemService(MediaProjectionManager::class.java).getMediaProjection(Activity.RESULT_OK, data)
            projection!!.registerCallback(callback, capture)
            running = true
            app.observer?.invoke()
            if (Build.VERSION.SDK_INT < 34) {
                getSystemService(DisplayManager::class.java).registerDisplayListener(displayListener, capture)
            }
            capture.post {
                try {
                    val (width, height) = dimensions(this)
                    resize(width, height)
                } catch (e: Exception) { fail(e) }
            }
            power.start()
            app.io.execute {
                try {
                    app.invoke("share", JSONObject().put("epoch", epoch))
                    if (!stopped && running) status(this, "sharing", "电脑现在可以申请连接，切到后台也可继续共享。")
                } catch (e: Exception) { fail(e, "coordinator") }
                try { app.refresh() } catch (_: Exception) { }
            }
            handler.post(poll)
        } catch (e: Exception) { fail(e) }
        return START_NOT_STICKY
    }
    private fun startAccessibility(): Int {
        if (Build.VERSION.SDK_INT < 30 || !app.ready || !RemotePreferences.accessibilityCapture(this, app.state) ||
            !RemotePreferences.wantsSharing(this, app.state) || !AccessibilityFrames.available(this)) {
            status(this, "interrupted", "辅助功能截图尚未授权，请在手机上恢复 FarSail 远程控制服务。")
            stopSelf(); return START_NOT_STICKY
        }
        try {
            startForeground(41, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
            epoch = NativeBridge.begin(); check(epoch >= 0) { "请等待初始化完成" }
            running = true; accessibilityMode = true; screenshotFrames = 0
            status(this, "starting", "正在通过辅助功能取得屏幕画面…")
            power.start()
            accessibilityFrames = AccessibilityFrames(this, capture, epoch, { width, height ->
                handler.post {
                    if (stopped) return@post
                    screenshotFrames++
                    lastFrame = SystemClock.elapsedRealtime()
                    screenWidth = width; screenHeight = height
                    if (!announced) {
                        announced = true
                        app.io.execute {
                            try {
                                app.invoke("share", JSONObject().put("epoch", epoch))
                                handler.post { if (!stopped) status(this, "sharing", "辅助功能截图共享中，无需重复确认录屏。受保护页面可能无法显示。") }
                                app.refresh()
                            } catch (e: Exception) { if (!stopped) fail(e, "coordinator") }
                        }
                    }
                }
            }, { if (!stopped) fail(it, "accessibility") }).also { it.start() }
            handler.post(poll)
        } catch (e: Exception) { fail(e, "accessibility") }
        return START_NOT_STICKY
    }
    private fun fail(e: Exception, stage: String = "capture") {
        NativeBridge.stopLocal()
        val code = Regex("(?:^|[^0-9])(401|403|409|429|500|502|503)(?:[^0-9]|$)").find(e.message.orEmpty())?.groupValues?.get(1) ?: e.javaClass.simpleName
        android.util.Log.w("FarSailSharing", "Sharing failed: stage=$stage code=$code")
        handler.post {
            if (stopped) return@post
            val reason = if (stage == "coordinator" && code == "409") "服务器拒绝安卓共享（409）。请升级服务器的安卓共享支持。" else app.friendly(e.message ?: "请重试。")
            status(this, "failed", reason)
            app.messageScope = "HOME"; app.message = "开启共享失败：$reason"
            stopSelf()
        }
    }
    private fun resize(w: Int, h: Int) {
        if (stopped || (screenWidth == w && screenHeight == h && display != null)) return
        screenWidth = w; screenHeight = h
        // Preserve native phone text detail, while keeping large displays bounded.
        val scale = minOf(1.0, 2560.0 / maxOf(w, h))
        val width = (w * scale).roundToInt().coerceAtLeast(1)
        val height = (h * scale).roundToInt().coerceAtLeast(1)
        layout = NativeBridge.geometry(width, height)
        val geometry = layout
        val old = reader
        val next = ImageReader.newInstance(width, height, PixelFormat.RGBA_8888, 2)
        reader = next
        next.setOnImageAvailableListener({ source ->
            val image = try { source.acquireLatestImage() } catch (_: Exception) { null } ?: return@setOnImageAvailableListener
            try {
                val now = SystemClock.elapsedRealtime()
                if (stopped || now - lastFrame < 100 || source !== reader) return@setOnImageAvailableListener
                lastFrame = now
                val plane = image.planes[0]
                check(plane.pixelStride == 4) { "不支持的录屏像素格式" }
                NativeBridge.publishRgba(plane.buffer.slice(), width, height, plane.rowStride, geometry)
            } catch (e: Exception) { fail(e) } finally { image.close() }
        }, capture)
        if (display == null) {
            display = projection!!.createVirtualDisplay("FarSail", width, height, resources.displayMetrics.densityDpi,
                DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, next.surface, null, capture)
        } else { display!!.surface = null; display!!.resize(width, height, resources.displayMetrics.densityDpi); display!!.surface = next.surface }
        old?.close()
    }
    private val poll = object : Runnable {
        override fun run() {
            if (stopped) return
            if (!polling) {
                polling = true
                app.io.execute {
                    try {
                        val now = SystemClock.elapsedRealtime()
                        if (!ConnectionService.running && now - lastHeartbeat >= 15000) { app.invoke("heartbeat"); lastHeartbeat = now }
                        app.refresh()
                    } catch (e: Exception) { app.messageScope = "HOME"; app.message = "连接检查失败：${e.message}" }
                    finally { handler.post {
                        polling = false
                        if (!stopped) {
                            if (Build.VERSION.SDK_INT < 33 || checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) == android.content.pm.PackageManager.PERMISSION_GRANTED) {
                                getSystemService(NotificationManager::class.java).notify(41, notification())
                            }
                            if (phase == "sharing" && !app.state.optBoolean("sharing")) { status(this@CaptureService, "failed", "服务器连接已停止，请重新开启共享。"); stopSelf() }
                        }
                        app.observer?.invoke()
                    } }
                }
            }
            handler.postDelayed(this, 2000)
        }
    }
    override fun onDestroy() {
        stopped = true; running = false; accessibilityMode = false
        if (Build.VERSION.SDK_INT >= 30) accessibilityFrames?.close()
        accessibilityFrames = null
        getSystemService(DisplayManager::class.java).unregisterDisplayListener(displayListener)
        if (phase != "failed" && phase != "interrupted") status(this, "off", "共享已停止，电脑无法继续查看。")
        NativeBridge.stopLocal(); ControlService.cancel()
        handler.removeCallbacks(poll)
        capture.post {
            display?.release(); display = null
            reader?.close(); reader = null
            projection?.unregisterCallback(callback); projection?.stop(); projection = null
            worker.quitSafely()
        }
        power.close()
        app.io.execute { try { app.invoke("stop") } catch (_: Exception) { }; try { app.refresh() } catch (_: Exception) { } }
        stopForeground(STOP_FOREGROUND_REMOVE)
        app.observer?.invoke()
        super.onDestroy()
    }
    override fun dump(fd: java.io.FileDescriptor, writer: java.io.PrintWriter, args: Array<out String>) {
        writer.println("deviceId=${app.state.optString("deviceId")}")
        writer.println("running=$running phase=$phase accessibilityMode=$accessibilityMode screenshotFrames=$screenshotFrames")
        writer.println("width=$screenWidth height=$screenHeight frameAgeMs=${if (lastFrame == 0L) -1 else SystemClock.elapsedRealtime() - lastFrame}")
        writer.println("controlServiceConnected=${ControlSetup.connected(this)} interactive=${getSystemService(PowerManager::class.java).isInteractive}")
        if (Build.VERSION.SDK_INT >= 30) accessibilityFrames?.let {
            writer.println("screenshotCaptureMs=${it.captureMs} screenshotAgeMs=${it.captureAgeMs} screenshotEncodeMs=${it.encodeMs} screenshotIntervalMs=${it.intervalMs}")
        }
        if (app.ready) runCatching { writer.println("media=${app.invoke("diagnostics")}") }
    }
    override fun onBind(intent: Intent?) = null
}

@Suppress("DEPRECATION")
fun dimensions(context: Context): Pair<Int, Int> {
    val wm = context.getSystemService(WindowManager::class.java)
    return if (Build.VERSION.SDK_INT >= 30) wm.maximumWindowMetrics.bounds.let { it.width() to it.height() }
    else android.util.DisplayMetrics().let { wm.defaultDisplay.getRealMetrics(it); it.widthPixels to it.heightPixels }
}
