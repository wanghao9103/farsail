package app.farsail.android

import android.app.KeyguardManager
import android.content.Intent
import android.os.*
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AccessibilitySharingTest {
    private val inst get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = inst.targetContext
    private val app get() = context.applicationContext as FarSailApp
    private val device get() = UiDevice.getInstance(inst)
    private fun shell(command: String) = ParcelFileDescriptor.AutoCloseInputStream(inst.getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES).executeShellCommand(command)).bufferedReader().use { it.readText().trim() }
    private fun await(condition: () -> Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 25000
        while (!condition() && SystemClock.elapsedRealtime() < deadline) Thread.sleep(100)
        assertTrue("accessibility sharing condition timed out", condition())
    }
    @Test fun explicitChoiceStreamsAcrossLockAndStopsWithoutRestoringAfterOptOut() {
        org.junit.Assume.assumeTrue("Owned API 30+ emulator only", Build.VERSION.SDK_INT >= 30 && shell("getprop ro.kernel.qemu") == "1" && Build.MODEL.contains("sdk", true))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        val keyguard = context.getSystemService(KeyguardManager::class.java)
        org.junit.Assume.assumeFalse("Never replace an existing credential", keyguard.isDeviceSecure)
        val oldServices = shell("settings get secure enabled_accessibility_services")
        val oldEnabled = shell("settings get secure accessibility_enabled")
        val oldDelay = shell("settings get secure lock_screen_lock_after_timeout")
        val oldOnline = ConnectionService.enabled(context)
        val coordinator = FakeCoordinator()
        var credentialSet = false
        try {
            device.wakeUp(); shell("wm dismiss-keyguard"); device.pressMenu(); await { app.ready }
            inst.runOnMainSync { ConnectionService.preference(context, false) }
            CaptureService.stop(context); await { !CaptureService.running && !ConnectionService.running }
            runCatching { app.invoke("logout") }; RemotePreferences.clear(context)
            app.invoke("server", JSONObject().put("url", "http://127.0.0.1:${coordinator.port}"))
            app.invoke("login", JSONObject().put("email", "screenshot@example.invalid").put("password", "synthetic-password"))
            app.invoke("bind", JSONObject().put("name", "Owned screenshot fixture"))
            app.io.submit { app.refresh() }.get(15, java.util.concurrent.TimeUnit.SECONDS)
            await { app.state.optString("server") == "http://127.0.0.1:${coordinator.port}" && app.state.optBoolean("signedIn") }
            assertFalse(RemotePreferences.accessibilityCapture(context, app.state))
            shell("settings put secure enabled_accessibility_services ''"); shell("settings put secure accessibility_enabled 0")
            await { ControlService.instance == null }
            shell("settings put secure enabled_accessibility_services app.farsail.android/app.farsail.android.ControlService")
            shell("settings put secure accessibility_enabled 1")
            await { AccessibilityFrames.available(context) }
            RemotePreferences.control(context, app.invoke("state") as JSONObject, true)
            inst.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            assertTrue(device.wait(Until.hasObject(By.text("启用免重复确认共享")), 10000))
            device.findObject(By.text("启用免重复确认共享")).click()
            assertTrue(device.wait(Until.hasObject(By.text("启用并共享")), 5000))
            assertFalse("Permission alone must not capture", CaptureService.running)
            device.findObject(By.text("启用并共享")).click()
            await { CaptureService.running && CaptureService.accessibilityMode && CaptureService.phase == "sharing" && CaptureService.screenshotFrames >= 3 }
            assertTrue(coordinator.hosting.get())
            assertTrue((app.invoke("state") as JSONObject).optBoolean("remoteWatch"))
            inst.runOnMainSync { ControlService.instance!!.onInterrupt() }
            Thread.sleep(750)
            assertTrue("Feedback interruption must not revoke a live system grant", CaptureService.running && (app.invoke("state") as JSONObject).optBoolean("control"))
            assertTrue("No MediaProjection session", shell("dumpsys media_projection").contains("null"))
            device.setOrientationLeft()
            await { CaptureService.screenWidth > CaptureService.screenHeight }
            device.setOrientationNatural()
            await { CaptureService.screenWidth < CaptureService.screenHeight }
            val frames = CaptureService.screenshotFrames
            check(shell("locksettings set-pin 1234").contains("set", true)); credentialSet = true
            shell("settings put secure lock_screen_lock_after_timeout 0")
            device.sleep(); await { keyguard.isKeyguardLocked && !context.getSystemService(PowerManager::class.java).isInteractive }
            Thread.sleep(10000)
            assertTrue("Screenshots must keep reaching native publisher while securely locked", CaptureService.screenshotFrames >= frames + 8)
            assertTrue(CaptureService.running && coordinator.hosting.get())
            check(shell("locksettings clear --old 1234").contains("cleared", true)); credentialSet = false
            device.wakeUp(); shell("wm dismiss-keyguard"); device.pressMenu(); await { !keyguard.isKeyguardLocked }
            val afterLock = CaptureService.screenshotFrames
            inst.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            await { CaptureService.screenshotFrames > afterLock + 2 }
            assertTrue("Returning to the app must preserve screenshot sharing", CaptureService.accessibilityMode)
            shell("settings put secure enabled_accessibility_services ''"); shell("settings put secure accessibility_enabled 0")
            await { ControlService.instance == null && !CaptureService.running && !coordinator.hosting.get() }
            assertTrue("System revocation retains only user intent", RemotePreferences.wantsSharing(context, app.invoke("state") as JSONObject))
            shell("settings put secure enabled_accessibility_services app.farsail.android/app.farsail.android.ControlService")
            shell("settings put secure accessibility_enabled 1")
            await { AccessibilityFrames.available(context) }
            inst.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            await { CaptureService.running && CaptureService.accessibilityMode && CaptureService.phase == "sharing" && CaptureService.screenshotFrames > 2 }
            assertTrue("Restoration must not require projection consent", shell("dumpsys media_projection").contains("null"))
            CaptureService.stop(context); await { !CaptureService.running && !coordinator.hosting.get() }
            val stoppedFrames = CaptureService.screenshotFrames
            Thread.sleep(1500); assertEquals(stoppedFrames, CaptureService.screenshotFrames)
            assertFalse(RemotePreferences.wantsSharing(context, app.invoke("state") as JSONObject))
            inst.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            Thread.sleep(1500); assertFalse("Explicit Stop must not auto-restore", CaptureService.running)
            // A different bound identity must not inherit this capture mode.
            val other = JSONObject(app.state.toString()).put("deviceId", "other-device")
            assertFalse(RemotePreferences.accessibilityCapture(context, other))
            RemotePreferences.clear(context)
            assertFalse(RemotePreferences.accessibilityCapture(context, app.state))
        } finally {
            CaptureService.stop(context); await { !CaptureService.running }
            if (credentialSet) shell("locksettings clear --old 1234")
            runCatching { app.invoke("logout") }; RemotePreferences.clear(context)
            app.io.submit { app.refresh() }.get(15, java.util.concurrent.TimeUnit.SECONDS)
            for ((key, value) in listOf("enabled_accessibility_services" to oldServices, "accessibility_enabled" to oldEnabled, "lock_screen_lock_after_timeout" to oldDelay))
                shell(if (value == "null") "settings delete secure $key" else "settings put secure $key '$value'")
            inst.runOnMainSync { ConnectionService.preference(context, oldOnline) }
            coordinator.close(); device.wakeUp(); shell("wm dismiss-keyguard"); device.unfreezeRotation(); device.pressHome()
        }
    }
}
