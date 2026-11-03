package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Two phases are driven by test-android-memory.py across an actual process death. */
@RunWith(AndroidJUnit4::class)
class RememberedAccessTest {
    private val inst get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = inst.targetContext
    private val app get() = context.applicationContext as FarSailApp
    private val device get() = UiDevice.getInstance(inst)
    private fun await(condition: () -> Boolean) {
        val end = System.currentTimeMillis() + 25000
        while (System.currentTimeMillis() < end) { if (condition()) return; Thread.sleep(100) }
        assertTrue("condition timed out", condition())
    }
    private fun shell(command: String): String = android.os.ParcelFileDescriptor.AutoCloseInputStream(inst.getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES).executeShellCommand(command)).bufferedReader().use { it.readText().trim() }
    private fun prepare() {
        org.junit.Assume.assumeTrue("External memory driver only", InstrumentationRegistry.getArguments().getString("memoryDriver") == "true")
        org.junit.Assume.assumeTrue("Owned emulator only", android.os.Build.MODEL.contains("sdk", ignoreCase = true) || android.os.Build.FINGERPRINT.contains("generic"))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        device.wakeUp(); shell("wm dismiss-keyguard"); await { app.ready }
    }
    private fun open() { context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) }
    private fun text(value: String): UiObject2 {
        val scroll = UiScrollable(UiSelector().scrollable(true))
        if (scroll.exists()) {
            scroll.scrollToBeginning(10)
            scroll.scrollIntoView(UiSelector().text(value))
        }
        device.waitForIdle(); Thread.sleep(300)
        return device.wait(Until.findObject(By.text(value)), 10000) ?: error("Missing UI: $value")
    }
    private fun grant() {
        val allow = device.wait(Until.findObject(By.res("android", "button1")), 12000)
        assertNotNull("fresh visible system projection consent", allow); allow.click()
        await { CaptureService.running && CaptureService.phase == "sharing" }
    }
    @Test fun userEnablesAndWaitsForProcessRestart() {
        prepare(); RemotePreferences.clear(context)
        val url = InstrumentationRegistry.getArguments().getString("fixtureUrl") ?: error("fixtureUrl required")
        assertTrue(url.startsWith("http://127.0.0.1:"))
        app.invoke("server", JSONObject().put("url", url))
        app.invoke("login", JSONObject().put("email", "memory-fixture@example.invalid").put("password", "synthetic-test-password"))
        app.invoke("bind", JSONObject().put("name", "Owned memory fixture")); app.refresh()
        open(); openSharingSettings(device)
        shell("settings put secure enabled_accessibility_services app.farsail.android/app.farsail.android.ControlService")
        shell("settings put secure accessibility_enabled 1"); await { ControlService.instance != null }
        val toggle = text("允许同账号设备直接控制").visibleBounds
        device.click(toggle.right - 50, toggle.centerY())
        await { (app.invoke("state") as JSONObject).optBoolean("control") }
        text("开启屏幕共享").click()
        val allow = device.wait(Until.findObject(By.res("android", "button1")), 12000); assertNotNull(allow)
        val positive = allow.visibleBounds; val cancel = device.findObject(By.res("android", "button2")); assertNotNull(cancel)
        val negative = cancel.visibleBounds; cancel.click(); await { CaptureService.phase == "off" }
        val start = text("开启屏幕共享"); val startBounds = start.visibleBounds; start.click(); grant()
        val stopBounds = text("停止共享").visibleBounds
        val buttons = JSONObject().put("cancel", org.json.JSONArray(listOf(negative.centerX(), negative.centerY())))
            .put("allow", org.json.JSONArray(listOf(positive.centerX(), positive.centerY())))
            .put("start", org.json.JSONArray(listOf(startBounds.centerX(), startBounds.centerY())))
            .put("stop", org.json.JSONArray(listOf(stopBounds.centerX(), stopBounds.centerY())))
        val state = app.invoke("state") as JSONObject
        assertTrue(RemotePreferences.wantsControl(context, state)); assertTrue(RemotePreferences.wantsSharing(context, state))
        val saved = java.io.File(context.applicationInfo.dataDir, "shared_prefs/remote_access.xml").readText()
        assertFalse(saved.contains("synthetic-test-password")); assertFalse(saved.contains("synthetic-refresh")); assertFalse(saved.contains("synthetic-access"))
        device.pressHome(); inst.waitForIdleSync()
        java.io.File(context.filesDir, "memory-ready.json").writeText(JSONObject().put("pid", android.os.Process.myPid()).put("sharing", true).put("control", true).put("buttons", buttons).toString())
        // The external driver kills this process while the original share is live.
        // Do not report a normal test pass or run a local-stop finally block.
        Thread.sleep(120000); fail("driver must restart the owned process")
    }
    @Test fun scopeAndLateIntentGuards() {
        prepare()
        try {
            open()
            // Instrumentation replaces the app class loader; explicitly rebind
            // this fixture. Normal-process restoration is tested by the provider.
            shell("settings put secure enabled_accessibility_services ''")
            shell("settings put secure accessibility_enabled 0")
            Thread.sleep(700); await { ControlService.instance == null }
            shell("settings put secure enabled_accessibility_services app.farsail.android/app.farsail.android.ControlService")
            shell("settings put secure accessibility_enabled 1"); await { ControlService.instance != null }
            val current = app.invoke("state") as JSONObject
            val old = RemotePreferences.ticket(); RemotePreferences.control(context, current, false)
            RemotePreferences.restoreControl(context, current, old)
            assertFalse((app.invoke("state") as JSONObject).optBoolean("control"))
            RemotePreferences.control(context, current, true)
            val foreign = JSONObject(current.toString()).put("deviceId", "40000000-0000-4000-8000-000000000002")
            RemotePreferences.restoreControl(context, foreign, RemotePreferences.ticket())
            assertFalse((app.invoke("state") as JSONObject).optBoolean("control"))
            RemotePreferences.restoreControl(context, app.invoke("state") as JSONObject, RemotePreferences.ticket())
            assertTrue((app.invoke("state") as JSONObject).optBoolean("control"))
            val captureTicket = RemotePreferences.beginConsent(context, current)
            RemotePreferences.stopSharing(context)
            assertFalse("late approval after stop is stale", RemotePreferences.sharing(context, current, captureTicket))
            val foreignTicket = RemotePreferences.beginConsent(context, current)
            assertFalse("new device cannot consume old consent", RemotePreferences.sharing(context, foreign, foreignTicket))
            val ticket = RemotePreferences.ticket(); RemotePreferences.clear(context)
            RemotePreferences.restoreControl(context, current, ticket)
            assertFalse((app.invoke("state") as JSONObject).optBoolean("control"))
        } finally {
            CaptureService.stop(context); RemotePreferences.clear(context)
            runCatching { app.invoke("logout") }
        }
    }
}
