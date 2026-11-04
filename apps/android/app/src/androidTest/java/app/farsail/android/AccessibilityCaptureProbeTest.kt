package app.farsail.android

import android.accessibilityservice.AccessibilityService
import android.app.KeyguardManager
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.os.*
import android.view.Display
import android.view.WindowManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class AccessibilityCaptureProbeTest {
    private val inst get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = inst.targetContext
    private val device get() = UiDevice.getInstance(inst)
    private fun shell(command: String) = ParcelFileDescriptor.AutoCloseInputStream(inst.getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES).executeShellCommand(command)).bufferedReader().use { it.readText().trim() }
    private fun await(condition: () -> Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 15000
        while (!condition() && SystemClock.elapsedRealtime() < deadline) Thread.sleep(100)
        assertTrue("probe condition timed out", condition())
    }
    private fun capture(stage: String): JSONObject {
        val service = checkNotNull(ControlService.instance)
        val done = CountDownLatch(1)
        val result = JSONObject().put("stage", stage)
        service.takeScreenshot(Display.DEFAULT_DISPLAY, context.mainExecutor, object : AccessibilityService.TakeScreenshotCallback {
            override fun onSuccess(value: AccessibilityService.ScreenshotResult) {
                val buffer = value.hardwareBuffer
                try {
                    val hardware = checkNotNull(Bitmap.wrapHardwareBuffer(buffer, value.colorSpace))
                    val bitmap = checkNotNull(hardware.copy(Bitmap.Config.ARGB_8888, false))
                    result.put("success", true).put("width", bitmap.width).put("height", bitmap.height)
                        .put("center", bitmap.getPixel(bitmap.width / 2, bitmap.height / 2))
                    val colors = (0 until 20).flatMap { x -> (0 until 20).map { y -> bitmap.getPixel(x * bitmap.width / 20, y * bitmap.height / 20) } }
                    result.put("sampleDistinctColors", colors.distinct().size)
                        .put("sampleVisiblePixels", colors.count { Color.red(it) > 10 || Color.green(it) > 10 || Color.blue(it) > 10 })
                    if (stage == "unlocked") {
                        val rgba = java.nio.ByteBuffer.allocate(bitmap.byteCount)
                        bitmap.copyPixelsToBuffer(rgba)
                        val offset = bitmap.height / 2 * bitmap.rowBytes + bitmap.width / 2 * 4
                        val pixel = (0..3).map { rgba.get(offset + it).toInt() and 255 }
                        result.put("centerRgba", JSONArray(pixel))
                        assertEquals("Bitmap memory must match the native RGBA contract", listOf(255, 0, 0, 255), pixel)
                    }
                    bitmap.recycle(); hardware.recycle()
                } finally { buffer.close(); done.countDown() }
            }
            override fun onFailure(errorCode: Int) { result.put("success", false).put("error", errorCode); done.countDown() }
        })
        assertTrue("screenshot callback timed out: $stage", done.await(10, TimeUnit.SECONDS))
        Thread.sleep(500) // Respect the platform screenshot interval.
        return result
    }
    @Test fun ordinaryScreenshotCapabilityAcrossSecureLockAndUnlock() {
        org.junit.Assume.assumeTrue("Owned API 30+ emulator only", Build.VERSION.SDK_INT >= 30 && shell("getprop ro.kernel.qemu") == "1" && Build.MODEL.contains("sdk", true))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        val keyguard = context.getSystemService(KeyguardManager::class.java)
        org.junit.Assume.assumeFalse("Never replace an existing credential", keyguard.isDeviceSecure)
        val oldEnabled = shell("settings get secure enabled_accessibility_services")
        val oldAccessibility = shell("settings get secure accessibility_enabled")
        val oldDelay = shell("settings get secure lock_screen_lock_after_timeout")
        val results = JSONArray()
        var credentialSet = false
        var activity: android.app.Activity? = null
        try {
            device.wakeUp(); shell("wm dismiss-keyguard"); device.pressMenu()
            shell("settings put secure enabled_accessibility_services ''"); shell("settings put secure accessibility_enabled 0")
            await { ControlService.instance == null }
            shell("settings put secure enabled_accessibility_services app.farsail.android/app.farsail.android.ControlService")
            shell("settings put secure accessibility_enabled 1")
            await { ControlSetup.connected(context) }
            val fixture = inst.startActivitySync(Intent(context, CaptureProbeActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            activity = fixture
            inst.waitForIdleSync(); Thread.sleep(500)
            val awake = capture("unlocked"); results.put(awake)
            assertTrue(awake.toString(), awake.optBoolean("success")); assertEquals(Color.RED, awake.getInt("center"))
            check(shell("locksettings set-pin 1234").contains("set", true)); credentialSet = true
            shell("settings put secure lock_screen_lock_after_timeout 0")
            device.sleep(); await { keyguard.isKeyguardLocked && !context.getSystemService(PowerManager::class.java).isInteractive }
            Thread.sleep(500)
            results.put(capture("asleep_secure_lock"))
            val gestureDone = CountDownLatch(1)
            val input = JSONObject().put("stage", "gesture_while_asleep")
            inst.runOnMainSync {
                val path = android.graphics.Path().apply { moveTo(540f, 1200f) }
                val gesture = android.accessibilityservice.GestureDescription.Builder().addStroke(android.accessibilityservice.GestureDescription.StrokeDescription(path, 0, 100)).build()
                input.put("accepted", ControlService.instance!!.dispatchGesture(gesture, object : AccessibilityService.GestureResultCallback() {
                    override fun onCompleted(gestureDescription: android.accessibilityservice.GestureDescription) { input.put("completed", true); gestureDone.countDown() }
                    override fun onCancelled(gestureDescription: android.accessibilityservice.GestureDescription) { input.put("completed", false); gestureDone.countDown() }
                }, null))
            }
            assertTrue("gesture callback", gestureDone.await(5, TimeUnit.SECONDS)); Thread.sleep(500)
            input.put("interactiveAfterGesture", context.getSystemService(PowerManager::class.java).isInteractive)
            results.put(input)
            val home = JSONObject().put("stage", "home_action_while_asleep")
            inst.runOnMainSync { home.put("accepted", ControlService.instance!!.performGlobalAction(AccessibilityService.GLOBAL_ACTION_HOME)) }
            Thread.sleep(750)
            home.put("interactiveAfterHome", context.getSystemService(PowerManager::class.java).isInteractive)
            results.put(home)
            device.sleep(); await { !context.getSystemService(PowerManager::class.java).isInteractive }
            inst.runOnMainSync { ControlService.instance!!.startActivity(Intent(context, WakeActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            Thread.sleep(750)
            assertFalse("Unapproved sessions must not wake the screen", context.getSystemService(PowerManager::class.java).isInteractive)
            results.put(JSONObject().put("stage", "unapproved_wake").put("interactive", false))
            // This debug-only subclass exercises window wake mechanics without
            // pretending that the probe has a real, approved native remote session.
            inst.runOnMainSync { ControlService.instance!!.startActivity(Intent(context, WakeProbeActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            await { context.getSystemService(PowerManager::class.java).isInteractive }
            Thread.sleep(750)
            assertTrue("Wake must not dismiss the secure keyguard", keyguard.isKeyguardLocked)
            results.put(JSONObject().put("stage", "wake_window_mechanics").put("interactive", true).put("secureStillLocked", true))
            device.wakeUp(); await { keyguard.isKeyguardLocked && context.getSystemService(PowerManager::class.java).isInteractive }
            Thread.sleep(500); results.put(capture("awake_secure_lock"))
            // Remove only this test's credential, then exercise an actual unlocked display.
            // This is not evidence of remote PIN entry or credential bypass.
            check(shell("locksettings clear --old 1234").contains("cleared", true)); credentialSet = false
            shell("wm dismiss-keyguard"); device.pressMenu()
            await { !keyguard.isKeyguardLocked }
            inst.runOnMainSync { fixture.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
            Thread.sleep(500)
            val restored = capture("unlocked_again_without_projection"); results.put(restored)
            assertTrue(restored.toString(), restored.optBoolean("success")); assertEquals(Color.RED, restored.getInt("center"))
            inst.runOnMainSync { fixture.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE) }
            Thread.sleep(500)
            val secure = capture("secure_window"); results.put(secure)
            assertTrue("Secure content must remain protected", !secure.optBoolean("success") || secure.optInt("center") != Color.RED)
        } finally {
            java.io.File(context.filesDir, "accessibility-capture-probe.json").writeText(results.toString(2))
            inst.runOnMainSync { activity?.finish() }
            if (credentialSet) shell("locksettings clear --old 1234")
            for ((key, value) in listOf("enabled_accessibility_services" to oldEnabled, "accessibility_enabled" to oldAccessibility, "lock_screen_lock_after_timeout" to oldDelay))
                shell(if (value == "null") "settings delete secure $key" else "settings put secure $key '$value'")
            device.wakeUp(); shell("wm dismiss-keyguard"); device.pressHome()
        }
    }
}
