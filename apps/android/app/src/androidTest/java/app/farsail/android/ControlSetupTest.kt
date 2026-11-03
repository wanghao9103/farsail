package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ControlSetupTest {
    @Test fun opensOrLocatesOurServiceWithoutChangingSystemConsent() {
        org.junit.Assume.assumeTrue("Dedicated emulator only", android.os.Build.MODEL.contains("sdk", ignoreCase = true))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        val inst = InstrumentationRegistry.getInstrumentation()
        inst.getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES).executeShellCommand("settings put secure enabled_accessibility_services ''").close()
        val context = inst.targetContext
        val app = context.applicationContext as FarSailApp
        val end = System.currentTimeMillis() + 15000
        while ((!app.ready || ControlService.instance != null) && System.currentTimeMillis() < end) Thread.sleep(100)
        assertNull(ControlService.instance)
        context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
        val device = UiDevice.getInstance(inst)
        openSharingSettings(device)
        val scroll = UiScrollable(UiSelector().scrollable(true))
        if (scroll.exists()) scroll.scrollIntoView(UiSelector().text("管理远程控制授权"))
        assertTrue(device.wait(Until.hasObject(By.text("管理远程控制授权")), 5000)); device.findObject(By.text("管理远程控制授权")).click()
        assertTrue(device.wait(Until.hasObject(By.text("去开启 FarSail 远程控制")), 5000))
        device.takeScreenshot(java.io.File(context.filesDir, "control-setup.png"))
        device.findObject(By.text("去开启 FarSail 远程控制")).click()
        assertTrue(device.wait(Until.hasObject(By.pkg("com.android.settings")), 10000))
        assertTrue("Our service details must open directly", device.wait(Until.hasObject(By.textContains("FarSail 远程控制")), 5000))
        device.takeScreenshot(java.io.File(context.filesDir, "control-system-details.png"))
        // Details may be signature/installer-protected. The supported fallback locates our named service row.
        assertNotNull(device.findObject(By.textContains("FarSail 远程控制")))
        device.takeScreenshot(java.io.File(context.filesDir, "control-system-details.png"))
        device.pressBack()
        assertTrue(device.wait(Until.hasObject(By.text("去开启 FarSail 远程控制")), 5000))
        assertFalse("Navigation is not system consent", context.getSystemService(android.view.accessibility.AccessibilityManager::class.java).getEnabledAccessibilityServiceList(-1).any { it.resolveInfo.serviceInfo.packageName == context.packageName })
    }
}
