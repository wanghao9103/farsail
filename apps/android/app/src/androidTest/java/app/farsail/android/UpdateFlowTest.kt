package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Dedicated emulator only. Uses the real public signed channel and normal system settings. */
@RunWith(AndroidJUnit4::class)
class UpdateFlowTest {
    @Test fun publicDownloadAndSystemInstaller() {
        org.junit.Assume.assumeTrue(InstrumentationRegistry.getArguments().getString("publicUpdate") == "true")
        val inst = InstrumentationRegistry.getInstrumentation()
        val context = inst.targetContext
        require(android.os.Build.FINGERPRINT.contains("generic") || android.os.Build.MODEL.contains("sdk", ignoreCase = true))
        val app = context.applicationContext as FarSailApp
        val device = UiDevice.getInstance(inst)
        val targetCode = InstrumentationRegistry.getArguments().getString("expectedVersionCode", "2").toLong()
        val verify = InstrumentationRegistry.getArguments().getString("phase") == "verify"
        if (verify) {
            assertEquals(targetCode, app.updates.currentCode)
            assertEquals("preserved", context.getSharedPreferences("upgrade-smoke", 0).getString("continuity", ""))
            java.io.File(context.filesDir, "update-verified.json").writeText("{\"installedVersion\":\"${app.updates.currentVersion}\",\"versionCode\":$targetCode,\"dataPreserved\":true}")
            return
        }
        assertEquals(1L, app.updates.currentCode)
        context.getSharedPreferences("upgrade-smoke", 0).edit().putString("continuity", "preserved").commit()
        context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
        assertTrue(device.wait(Until.hasObject(By.text("设置")), 15000))
        device.findObject(By.text("设置")).click()
        assertTrue(device.wait(Until.hasObject(By.desc("软件更新")), 5000))
        device.findObject(By.desc("软件更新")).click()
        inst.runOnMainSync { app.updates.check() }
        inst.waitForIdleSync()
        val deadline = System.currentTimeMillis() + 180000
        while (app.updates.state.phase == "checking" || app.updates.state.phase == "idle") { assertTrue("check timed out", System.currentTimeMillis() < deadline); Thread.sleep(200) }
        val requireAutomatic = InstrumentationRegistry.getArguments().getString("automaticDownload") == "true"
        if (requireAutomatic) assertTrue("automatic downloads enabled", app.updates.autoDownload)
        if (!requireAutomatic && app.updates.state.phase == "available") {
            assertTrue(device.wait(Until.hasObject(By.text("下载更新")), 5000))
            device.findObject(By.text("下载更新")).click()
        }
        while (app.updates.state.phase in listOf("checking", "available", "downloading")) { assertTrue("download timed out", System.currentTimeMillis() < deadline); Thread.sleep(200) }
        assertEquals(app.updates.state.message, "ready", app.updates.state.phase)
        assertTrue(device.wait(Until.hasObject(By.text("安装更新")), 5000))
        var permissionUi = false
        device.findObject(By.text("安装更新")).click()
        if (!context.packageManager.canRequestPackageInstalls()) {
            assertTrue(device.wait(Until.hasObject(By.checkable(true)), 10000))
            device.findObject(By.checkable(true)).click()
            permissionUi = true
            device.pressBack()
            assertTrue(device.wait(Until.hasObject(By.text("安装更新")), 5000))
            device.findObject(By.text("安装更新")).click()
        }
        assertTrue(device.wait(Until.hasObject(By.pkg("com.android.packageinstaller")), 15000) || device.wait(Until.hasObject(By.pkg("com.google.android.packageinstaller")), 15000))
        device.takeScreenshot(java.io.File(context.filesDir, "update-system-installer.png"))
        java.io.File(context.filesDir, "update-prepared.json").writeText("{\"publicDownload\":true,\"automaticDownload\":$requireAutomatic,\"signedMetadata\":true,\"apkVerified\":true,\"normalPermissionUI\":$permissionUi,\"installerOpened\":true}")
    }
}
