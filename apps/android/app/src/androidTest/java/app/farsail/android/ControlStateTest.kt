package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Actual Settings changes and JNI state on the dedicated emulator, never a personal phone. */
@RunWith(AndroidJUnit4::class)
class ControlStateTest {
    private val inst get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = inst.targetContext
    private val app get() = context.applicationContext as FarSailApp
    private val device get() = UiDevice.getInstance(inst)
    private fun await(test: () -> Boolean) {
        val end = System.currentTimeMillis() + 20000
        while (System.currentTimeMillis() < end) { if (test()) return; Thread.sleep(100) }
        assertTrue("control state condition timed out", test())
    }
    private fun shell(cmd: String) = android.os.ParcelFileDescriptor.AutoCloseInputStream(inst.getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES).executeShellCommand(cmd)).bufferedReader().use { it.readText().trim() }
    private fun systemControl(enabled: Boolean) {
        shell("settings put secure enabled_accessibility_services ${if (enabled) "app.farsail.android/app.farsail.android.ControlService" else "''"}")
        shell("settings put secure accessibility_enabled ${if (enabled) 1 else 0}")
        await { ControlSetup.connected(context) == enabled }
    }
    private fun open() { context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    private fun background() {
        device.pressHome()
        // A foreground retry requires a completed pause, not a cancelled transition.
        await { app.observer == null }
        assertTrue(device.wait(Until.gone(By.pkg(context.packageName)), 10000))
    }
    private fun text(value: String): UiObject2 {
        val scroll = UiScrollable(UiSelector().scrollable(true))
        if (scroll.exists()) { scroll.scrollToBeginning(10); scroll.scrollIntoView(UiSelector().text(value)) }
        device.waitForIdle()
        return device.wait(Until.findObject(By.text(value)), 5000) ?: error("Missing control UI: $value")
    }
    private fun fixture(rejectSharing: Boolean = false, block: (FakeCoordinator) -> Unit) {
        org.junit.Assume.assumeTrue("Dedicated memory emulator only", shell("getprop ro.kernel.qemu") == "1" && shell("getprop ro.product.model").contains("sdk", true))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        val oldEnabled = shell("settings get secure accessibility_enabled")
        val oldServices = shell("settings get secure enabled_accessibility_services")
        device.wakeUp(); shell("wm dismiss-keyguard"); device.pressMenu()
        await { context.getSystemService(android.os.PowerManager::class.java).isInteractive && !context.getSystemService(android.app.KeyguardManager::class.java).isKeyguardLocked }
        await { app.ready }
        val coordinator = FakeCoordinator(rejectSharing)
        try {
            CaptureService.stop(context); await { !CaptureService.running }
            runCatching { app.invoke("logout") }; RemotePreferences.clear(context); systemControl(false)
            app.invoke("server", JSONObject().put("url", "http://127.0.0.1:${coordinator.port}"))
            app.invoke("login", JSONObject().put("email", "state@example.invalid").put("password", "synthetic-password"))
            app.invoke("bind", JSONObject().put("name", "Control state fixture"))
            // Drain lifecycle refreshes before using the published UI snapshot.
            app.io.submit { app.refresh() }.get(15, java.util.concurrent.TimeUnit.SECONDS)
            await { app.state.optBoolean("signedIn") && app.state.optString("deviceId").startsWith("40000000") && app.state.optString("server") == "http://127.0.0.1:${coordinator.port}" }
            context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            if (!device.wait(Until.hasObject(By.text("设置")), 10000)) {
                device.dumpWindowHierarchy(java.io.File(context.filesDir, "state-launch-failure.xml"))
                device.takeScreenshot(java.io.File(context.filesDir, "state-launch-failure.png"))
                fail("Fixture did not reach Home after authenticated setup")
            }
            block(coordinator)
        } finally {
            inst.runOnMainSync { app.busy = false }
            CaptureService.stop(context); RemotePreferences.clear(context); await { !CaptureService.running }
            systemControl(false); runCatching { app.invoke("logout") }
            shell(if (oldServices == "null") "settings delete secure enabled_accessibility_services" else "settings put secure enabled_accessibility_services '$oldServices'")
            shell(if (oldEnabled == "null") "settings delete secure accessibility_enabled" else "settings put secure accessibility_enabled '$oldEnabled'")
            coordinator.close(); device.pressHome()
        }
    }
    @Test fun systemRevocationUpdatesSwitchAndRestoresSameAccountApproval() = fixture { coordinator ->
        openSharingSettings(device)
        RemotePreferences.control(context, app.invoke("state") as JSONObject, true); app.refresh()
        await { app.state.has("control") }
        assertFalse("saved intent alone must not turn on the switch", text("允许同账号设备直接控制").isChecked)
        assertNotNull(text("取消已记住的控制设置"))
        assertTrue(device.wait(Until.hasObject(By.textContains("系统辅助功能已关闭")), 5000))
        systemControl(true)
        await { (app.invoke("state") as JSONObject).optBoolean("control") }
        await { text("允许同账号设备直接控制").isChecked }
        text("开启屏幕共享").click()
        val consent = device.wait(Until.findObject(By.res("android", "button1")), 10000)
        assertNotNull("fresh system capture consent", consent); consent.click()
        await { CaptureService.phase == "sharing" }
        coordinator.requestReady.set(true)
        await { coordinator.approved.get() }
        assertFalse(coordinator.foreignApproved.get())
        background(); systemControl(false)
        await { val state = app.invoke("state") as JSONObject; !state.optBoolean("control") && !state.optBoolean("remoteWatch") }
        assertTrue("revoked control must not erase the saved choice", RemotePreferences.wantsControl(context, app.invoke("state") as JSONObject))
        assertTrue("valid capture can continue without control", CaptureService.running)
        coordinator.approved.set(false); coordinator.requestReady.set(true)
        Thread.sleep(2500); assertFalse("no auto approval after revocation", coordinator.approved.get())
        open()
        assertFalse(text("允许同账号设备直接控制").isChecked)
        systemControl(true)
        await { (app.invoke("state") as JSONObject).optBoolean("control") && coordinator.approved.get() }
        assertFalse("other accounts still require approval", coordinator.foreignApproved.get())
        systemControl(false); open(); text("取消已记住的控制设置").click()
        assertFalse(RemotePreferences.wantsControl(context, app.invoke("state") as JSONObject))
        systemControl(true); Thread.sleep(500)
        assertFalse("restored system permission cannot undo explicit opt out", (app.invoke("state") as JSONObject).optBoolean("control"))
    }
    @Test fun rememberedSharingStartsFromSettingsAndRetriesFailureOnNextEntry() = fixture {
        // Queue startup while busy; navigating to Settings must not lose the attempt.
        inst.runOnMainSync { app.busy = true }
        val ticket = RemotePreferences.beginConsent(context, app.state)
        assertTrue(ticket > 0); RemotePreferences.abandonConsent(context, ticket)
        val settings = device.wait(Until.findObject(By.text("设置")), 10000)
        assertNotNull(settings); settings.click()
        inst.runOnMainSync { app.busy = false; app.observer?.invoke() }
        val consent = device.wait(Until.findObject(By.res("android", "button1")), 10000)
        assertNotNull("settings startup requests capture without an app Start click", consent)
        device.findObject(By.res("android", "button2")).click()
        await { !RemotePreferences.wantsSharing(context, app.state) }
        background(); open(); Thread.sleep(500)
        assertFalse("cancel does not loop on foreground", device.hasObject(By.res("android", "button1")))
        val retry = RemotePreferences.beginConsent(context, app.state); RemotePreferences.abandonConsent(context, retry)
        inst.runOnMainSync { CaptureService.status(context, "failed", "Synthetic previous startup failure") }
        background(); open()
        val again = device.wait(Until.findObject(By.res("android", "button1")), 10000)
        assertNotNull("next foreground retries a saved failed startup without Start", again); again.click()
        await { CaptureService.phase == "sharing" }
        background(); open(); Thread.sleep(500)
        assertTrue(CaptureService.running)
        assertFalse("existing capture never asks again", device.hasObject(By.res("android", "button1")))
    }
    @Test fun failedAutomaticStartDoesNotRepromptUntilNextForeground() = fixture(true) {
        val ticket = RemotePreferences.beginConsent(context, app.state); RemotePreferences.abandonConsent(context, ticket)
        inst.runOnMainSync { app.observer?.invoke() }
        val consent = device.wait(Until.findObject(By.res("android", "button1")), 10000)
        assertNotNull(consent); consent.click()
        await { !CaptureService.running && CaptureService.phase == "failed" }
        Thread.sleep(1000)
        assertTrue(RemotePreferences.wantsSharing(context, app.state))
        assertFalse("failure must not trigger another permission dialog in the same visit", device.hasObject(By.res("android", "button1")))
        background(); open()
        assertNotNull("a later foreground can retry", device.wait(Until.findObject(By.res("android", "button1")), 10000))
        device.findObject(By.res("android", "button2")).click()
        await { !RemotePreferences.wantsSharing(context, app.state) }
    }
}
