package app.farsail.android

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.AccessibilityServiceInfo
import android.content.Context
import android.graphics.Bitmap
import android.os.Build
import android.os.Handler
import android.os.SystemClock
import android.view.Display
import androidx.annotation.RequiresApi
import java.nio.ByteBuffer
import kotlin.math.roundToInt

/** A user-selected capture backend; the OS accessibility grant remains authoritative. */
@RequiresApi(30)
class AccessibilityFrames(
    private val context: Context,
    private val worker: Handler,
    private val epoch: Long,
    private val onFrame: (Int, Int) -> Unit,
    private val onFailure: (Exception) -> Unit,
) : AutoCloseable {
    companion object {
        fun available(context: Context) = Build.VERSION.SDK_INT >= 30 && ControlSetup.connected(context) &&
            ControlService.instance?.serviceInfo?.capabilities?.let { it and AccessibilityServiceInfo.CAPABILITY_CAN_TAKE_SCREENSHOT != 0 } == true
    }
    @Volatile private var closed = false
    private var buffer: ByteBuffer? = null
    @Volatile var captureMs = 0L
        private set
    @Volatile var encodeMs = 0L
        private set
    @Volatile var intervalMs = 350L
        private set
    private val timeout = Runnable { fail(IllegalStateException("辅助功能截图超时，请检查后台运行设置。")) }
    private fun fail(e: Exception) { if (!closed) { close(); onFailure(e) } }
    fun start() { worker.post(tick) }
    private val tick: Runnable = object : Runnable {
        override fun run() {
            if (closed) return
            if (!available(context)) { fail(IllegalStateException("辅助功能截图权限或服务已断开，请在手机设置中恢复 FarSail 远程控制授权。")); return }
            val requestedAt = SystemClock.uptimeMillis()
            worker.postDelayed(timeout, 10000)
            try {
                ControlService.instance!!.takeScreenshot(Display.DEFAULT_DISPLAY, { task -> if (!worker.post(task)) task.run() }, object : AccessibilityService.TakeScreenshotCallback {
                    override fun onSuccess(result: AccessibilityService.ScreenshotResult) {
                        worker.removeCallbacks(timeout)
                        captureMs = SystemClock.uptimeMillis() - requestedAt
                        val hardware = result.hardwareBuffer
                        var original: Bitmap? = null
                        var pixels: Bitmap? = null
                        var scaled: Bitmap? = null
                        try {
                            if (closed) return
                            original = checkNotNull(Bitmap.wrapHardwareBuffer(hardware, result.colorSpace))
                            pixels = checkNotNull(original.copy(Bitmap.Config.ARGB_8888, false))
                            val width = pixels.width; val height = pixels.height
                            val scale = minOf(1.0, 2560.0 / maxOf(width, height))
                            scaled = if (scale < 1) Bitmap.createScaledBitmap(pixels, (width * scale).roundToInt().coerceAtLeast(1), (height * scale).roundToInt().coerceAtLeast(1), true) else pixels
                            val bytes = scaled.byteCount
                            val rgba = buffer?.takeIf { it.capacity() >= bytes } ?: ByteBuffer.allocateDirect(bytes).also { buffer = it }
                            rgba.clear(); scaled.copyPixelsToBuffer(rgba); rgba.rewind()
                            val encodeAt = SystemClock.uptimeMillis()
                            val accepted = !closed && NativeBridge.publishAccessibilityRgba(rgba, scaled.width, scaled.height, scaled.rowBytes, epoch)
                            encodeMs = SystemClock.uptimeMillis() - encodeAt
                            if (accepted) onFrame(width, height)
                        } catch (e: Exception) { fail(e) }
                        finally {
                            if (scaled !== pixels) scaled?.recycle()
                            pixels?.recycle(); original?.recycle(); hardware.close()
                            // Request cadence includes capture/copy/encoding time; do
                            // not add a fixed half-second after all that work finishes.
                            if (!closed) worker.postDelayed(this@AccessibilityFrames.tick, (intervalMs - (SystemClock.uptimeMillis() - requestedAt)).coerceAtLeast(0))
                        }
                    }
                    override fun onFailure(errorCode: Int) {
                        worker.removeCallbacks(timeout)
                        if (closed) return
                        if (errorCode == AccessibilityService.ERROR_TAKE_SCREENSHOT_INTERVAL_TIME_SHORT) {
                            intervalMs = (intervalMs + 100).coerceAtMost(1500)
                            worker.postDelayed(this@AccessibilityFrames.tick, intervalMs)
                        }
                        else fail(IllegalStateException("系统暂不允许辅助功能截图（$errorCode），请检查屏幕保护或辅助功能授权。"))
                    }
                })
            } catch (e: Exception) { worker.removeCallbacks(timeout); fail(e) }
        }
    }
    override fun close() { closed = true; worker.removeCallbacks(tick); worker.removeCallbacks(timeout); buffer = null }
}
