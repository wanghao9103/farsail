package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** First authorization and delayed account initialization; dedicated emulator only. */
@RunWith(AndroidJUnit4::class)
class AccessChoiceTest {
    private val inst get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = inst.targetContext
    private val app get() = context.applicationContext as FarSailApp
    private val device get() = UiDevice.getInstance(inst)
    private fun await(test: () -> Boolean) {
        val end = System.currentTimeMillis() + 25000
        while (System.currentTimeMillis() < end) { if (test()) return; Thread.sleep(100) }
        assertTrue("access choice condition timed out", test())
    }
    private fun shell(cmd: String) = android.os.ParcelFileDescriptor.AutoCloseInputStream(inst.getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES).executeShellCommand(cmd)).bufferedReader().use { it.readText().trim() }
    private fun prepare() {
        org.junit.Assume.assumeTrue("External choice driver only", InstrumentationRegistry.getArguments().getString("choiceDriver") == "true")
        org.junit.Assume.assumeTrue("Owned emulator only", android.os.Build.MODEL.contains("sdk",true) || android.os.Build.FINGERPRINT.contains("generic"))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        device.wakeUp(); shell("wm dismiss-keyguard"); await { app.ready }
        shell("settings put secure enabled_accessibility_services ''"); shell("settings put secure accessibility_enabled 0")
        await { ControlService.instance == null }
        runCatching { app.invoke("logout") }; RemotePreferences.clear(context)
        val url = InstrumentationRegistry.getArguments().getString("fixtureUrl") ?: error("fixtureUrl required")
        assertTrue(url.matches(Regex("http://127\\.0\\.0\\.1:[0-9]+")))
        app.invoke("server",JSONObject().put("url",url))
        app.invoke("login",JSONObject().put("email","choice@example.invalid").put("password","synthetic-choice-password"))
        app.invoke("bind",JSONObject().put("name","Owned choice fixture")); app.refresh()
        await { app.state.optBoolean("signedIn") && app.state.optString("deviceId").let { it.isNotBlank() && it != "null" } }
        context.startActivity(Intent(context,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
        openSharingSettings(device)
    }
    private fun text(value: String): UiObject2 {
        val scroll = UiScrollable(UiSelector().scrollable(true))
        if (scroll.exists()) scroll.scrollIntoView(UiSelector().text(value))
        device.waitForIdle(); Thread.sleep(300)
        return device.wait(Until.findObject(By.text(value)),5000) ?: error("Missing choice UI: $value")
    }
    @Test fun controlChoiceSurvivesPendingSystemAuthorization() {
        prepare()
        val toggle = text("允许同账号设备直接控制")
        device.dumpWindowHierarchy(java.io.File(context.filesDir,"choice-before-click.xml"))
        val bounds = toggle.visibleBounds
        assertTrue(device.click(bounds.right - 50,bounds.centerY()))
        device.waitForIdle()
        await { RemotePreferences.wantsControl(context,app.invoke("state") as JSONObject) }
        val state = app.invoke("state") as JSONObject
        assertTrue("Allow control must be saved before accessibility connects",RemotePreferences.wantsControl(context,state))
        assertFalse("No system permission means no runtime control",state.optBoolean("control"))
        java.io.File(context.filesDir,"choice-pending.json").writeText(JSONObject().put("pid",android.os.Process.myPid()).put("wantedControl",true).toString())
        // The driver replaces the APK here, losing this Activity and its pending flag.
        Thread.sleep(120000); fail("Driver must replace the owned process")
    }
    private fun projectionDuringInitialization(cancel: Boolean) {
        prepare()
        val state = app.invoke("state") as JSONObject
        val start = text("开启屏幕共享"); val startBounds = start.visibleBounds; start.click()
        val allow = device.wait(Until.findObject(By.res("android","button1")),12000)
        assertNotNull("Visible fresh system recording consent",allow)
        assertTrue("Share choice must be saved before recording consent returns",RemotePreferences.wantsSharing(context,state))
        val blocked = CountDownLatch(1); val release = CountDownLatch(1)
        app.io.execute { blocked.countDown(); release.await(12,TimeUnit.SECONDS) }
        assertTrue(blocked.await(3,TimeUnit.SECONDS))
        try {
            inst.runOnMainSync { app.ready = false; app.state = JSONObject() }
            allow.click(); Thread.sleep(700)
            assertFalse("Do not start recording before account state is ready",CaptureService.running)
            if (cancel) CaptureService.stop(context)
            inst.runOnMainSync { app.state = state; app.ready = true; app.observer?.invoke() }
            release.countDown()
            if (cancel) {
                Thread.sleep(1200)
                assertFalse("A late positive result cannot undo Stop",CaptureService.running)
                assertFalse(RemotePreferences.wantsSharing(context,state))
            } else {
                await { CaptureService.running && CaptureService.phase == "sharing" }
                assertTrue(RemotePreferences.wantsSharing(context,state))
                val marker = JSONObject().put("pid",android.os.Process.myPid()).put("sharing",true)
                    .put("startX",startBounds.centerX()).put("startY",startBounds.centerY())
                java.io.File(context.filesDir,"choice-sharing.json").writeText(marker.toString())
                device.pressHome(); Thread.sleep(300); assertTrue(CaptureService.running)
            }
        } finally {
            release.countDown()
            inst.runOnMainSync { app.ready = true; app.state = state }
            CaptureService.stop(context); RemotePreferences.clear(context)
            runCatching { app.invoke("logout") }; device.pressHome()
        }
    }
    @Test fun grantedProjectionWaitsForRestoredAccountState() = projectionDuringInitialization(false)
    @Test fun stopRejectsDeferredProjectionResult() = projectionDuringInitialization(true)
}
