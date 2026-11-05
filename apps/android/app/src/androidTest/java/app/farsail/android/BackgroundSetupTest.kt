package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BackgroundSetupTest {
    @Test fun settingsGuideAndAppDetailsPreserveUserChoices() {
        val inst = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(inst)
        org.junit.Assume.assumeTrue("Owned emulator only", android.os.Build.MODEL.contains("sdk", true))
        device.wakeUp(); device.pressMenu()
        val context = inst.targetContext
        val exempt = BackgroundSetup.exempt(context)
        val online = ConnectionService.enabled(context)
        val control = ControlSetup.enabled(context)
        context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
        val settings = device.wait(Until.findObject(By.text("设置")), 10000)
        assertNotNull(settings); settings.click()
        assertTrue(device.wait(Until.hasObject(By.desc("共享这台手机")), 5000))
        device.waitForIdle()
        val scroll = UiScrollable(UiSelector().scrollable(true))
        if (scroll.exists()) scroll.scrollIntoView(UiSelector().description("锁屏与后台设置"))
        val guide = device.wait(Until.findObject(By.desc("锁屏与后台设置")), 5000)
        assertNotNull(guide); guide.click()
        assertTrue(device.wait(Until.hasObject(By.text("设置电池优化豁免")), 5000))
        device.takeScreenshot(java.io.File(context.filesDir, "background-settings.png"))
        device.findObject(By.text("打开应用后台设置")).click()
        assertTrue(device.wait(Until.hasObject(By.pkg("com.android.settings")), 10000))
        assertTrue(device.wait(Until.hasObject(By.textContains("FarSail")), 5000))
        device.pressBack()
        assertTrue(device.wait(Until.hasObject(By.text("设置电池优化豁免")), 5000))
        if (scroll.exists()) scroll.scrollIntoView(UiSelector().text("管理辅助功能授权"))
        assertTrue(device.wait(Until.hasObject(By.textContains(if (android.os.Build.VERSION.SDK_INT < 30) "安卓 10 及以下不支持" else "安卓 11 及以上可")), 5000))
        device.findObject(By.text("管理辅助功能授权")).click()
        assertTrue(device.wait(Until.hasObject(By.text("去开启 FarSail 远程控制")), 5000))
        device.pressBack()
        assertTrue(device.wait(Until.hasObject(By.text("锁屏与后台设置")), 5000))
        assertEquals(exempt, BackgroundSetup.exempt(context))
        assertEquals(online, ConnectionService.enabled(context))
        assertEquals(control, ControlSetup.enabled(context))
        device.pressBack(); device.pressHome()
    }

    @Test fun unavailableOrProtectedDestinationsFallBackWithoutGrantingAnything() {
        val attempts = mutableListOf<String>()
        assertTrue(BackgroundSetup.launch(listOf(Intent("missing"), Intent("protected"), Intent("details"))) {
            attempts.add(it.action!!)
            when (it.action) {
                "missing" -> throw android.content.ActivityNotFoundException()
                "protected" -> throw SecurityException()
            }
        })
        assertEquals(listOf("missing", "protected", "details"), attempts)
        assertFalse(BackgroundSetup.launch(listOf(Intent("missing"))) { throw android.content.ActivityNotFoundException() })
        assertTrue(BackgroundSetup.colorOs("OPPO", "OPPO"))
        assertTrue(BackgroundSetup.colorOs("OnePlus", "OnePlus"))
        assertFalse(BackgroundSetup.colorOs("Google", "google"))
        assertTrue(BackgroundSetup.captureDetail(29).contains("不支持免重复确认"))
        assertTrue(BackgroundSetup.captureDetail(30).contains("需手动授权"))
    }

    @Test fun opensSystemExemptionConsentWithoutGrantingIt() {
        val inst = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(inst)
        org.junit.Assume.assumeTrue("Owned emulator only", android.os.Build.MODEL.contains("sdk", true))
        device.wakeUp(); device.pressMenu()
        val context = inst.targetContext
        val before = BackgroundSetup.exempt(context)
        org.junit.Assume.assumeFalse("This case needs an unexempted emulator app", before)
        val activity = inst.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
        try {
            inst.runOnMainSync { BackgroundSetup.open(activity) }
            assertTrue("System battery consent must open", device.wait(Until.hasObject(By.pkg("com.android.settings")), 10000))
            assertTrue("The system must name the app", device.wait(Until.hasObject(By.textContains("FarSail")), 5000))
            assertFalse("Opening the dialog must not grant exemption", BackgroundSetup.exempt(context))
            device.pressBack()
            assertFalse("Dismissing must preserve the user's choice", BackgroundSetup.exempt(context))
        } finally { device.pressBack(); device.pressHome() }
    }
}
