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
    private var stableFrames = 0
    @Volatile var captureMs = 0L
        private set
    @Volatile var captureAgeMs = 0L
        private set
    @Volatile var encodeMs = 0L
        private set
    @Volatile var intervalMs = 350L
        private set
    private var requestGeneration = 0L
    private var timeoutStreak = 0
    private val timeout = Runnable {
        if (!closed) {
            // A system screenshot callback can be lost during display transitions.
            // Fence the abandoned request before retrying; it may still return later.
            requestGeneration++
            if (available(context) && ++timeoutStreak <= 2) {
                android.util.Log.w("FarSailSharing", "Screenshot timed out; retry=$timeoutStreak")
                worker.postDelayed(tick, intervalMs)
            } else {
                android.util.Log.w("FarSailSharing", "Screenshot timed out; retries exhausted or grant lost")
                fail(IllegalStateException("辅助功能截图超时，请检查后台运行设置。"))
            }
        }
    }
    private fun fail(e: Exception) { if (!closed) { close(); onFailure(e) } }
    fun start() { worker.post(tick) }
    private val tick: Runnable = object : Runnable {
        override fun run() {
            if (closed) return
            if (!available(context)) { fail(IllegalStateException("辅助功能截图权限或服务已断开，请在手机设置中恢复 FarSail 远程控制授权。")); return }
            val requestedAt = SystemClock.uptimeMillis()
            val request = ++requestGeneration
            worker.postDelayed(timeout, 10000)
            try {
                ControlService.instance!!.takeScreenshot(Display.DEFAULT_DISPLAY, { task -> if (!worker.post(task)) task.run() }, object : AccessibilityService.TakeScreenshotCallback {
                    override fun onSuccess(result: AccessibilityService.ScreenshotResult) {
                        if (closed || request != requestGeneration) { result.hardwareBuffer.close(); return }
                        worker.removeCallbacks(timeout)
                        timeoutStreak = 0
                        captureMs = SystemClock.uptimeMillis() - requestedAt
                        captureAgeMs = (SystemClock.uptimeMillis() - result.timestamp).coerceAtLeast(0)
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
                            if (accepted) {
                                onFrame(width, height)
                                // A temporary rate limit must not permanently slow the session.
                                if (++stableFrames >= 30) { stableFrames = 0; intervalMs = (intervalMs - 50).coerceAtLeast(350) }
                            }
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
                        if (closed || request != requestGeneration) return
                        worker.removeCallbacks(timeout)
                        if (errorCode == AccessibilityService.ERROR_TAKE_SCREENSHOT_INTERVAL_TIME_SHORT) {
                            stableFrames = 0
                            intervalMs = (intervalMs + 100).coerceAtMost(1500)
                            worker.postDelayed(this@AccessibilityFrames.tick, intervalMs)
                        }
                        else {
                            android.util.Log.w("FarSailSharing", "Screenshot rejected: code=$errorCode")
                            fail(IllegalStateException("系统暂不允许辅助功能截图（$errorCode），请检查屏幕保护或辅助功能授权。"))
                        }
                    }
                })
            } catch (e: Exception) { worker.removeCallbacks(timeout); fail(e) }
        }
    }
    override fun close() { closed = true; worker.removeCallbacks(tick); worker.removeCallbacks(timeout); buffer = null }
}
