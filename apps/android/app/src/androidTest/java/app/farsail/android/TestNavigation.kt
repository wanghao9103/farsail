package app.farsail.android

import androidx.test.uiautomator.*
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue

/** Follow the same settings route as a user; no test-only Activity intent. */
internal fun openSharingSettings(device: UiDevice) {
    val settings = device.wait(Until.findObject(By.text("设置")), 10000)
    assertNotNull("Home exposes settings", settings); settings.click()
    val item = device.wait(Until.findObject(By.desc("共享这台手机")), 5000)
    assertNotNull("Sharing belongs to settings", item); item.click()
    assertTrue(device.wait(Until.hasObject(By.text("允许其他设备查看或控制本机。")), 5000))
    device.waitForIdle()
}
