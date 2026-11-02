package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AuthFlowTest {
    @Test fun loginServerRegistrationAndVerificationHaveSeparateForms() {
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        val inst = InstrumentationRegistry.getInstrumentation(); val context = inst.targetContext
        val app = context.applicationContext as FarSailApp
        val deadline = System.currentTimeMillis() + 15000
        while (!app.ready && System.currentTimeMillis() < deadline) Thread.sleep(100)
        runCatching { app.invoke("logout") }; app.refresh()
        context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
        val device = UiDevice.getInstance(inst)
        assertTrue(device.wait(Until.hasObject(By.desc("邮箱")), 10000))
        device.takeScreenshot(java.io.File(context.filesDir, "ui-login.png"))
        assertNotNull(device.findObject(By.desc("密码")))
        assertNull(device.findObject(By.desc("服务地址")))
        assertNull(device.findObject(By.desc("验证码")))
        device.findObject(By.desc("邮箱")).text = "flow-test@example.invalid"
        device.findObject(By.text("设置")).click()
        assertTrue(device.wait(Until.hasObject(By.desc("服务器设置")), 5000))
        device.findObject(By.desc("服务器设置")).click()
        assertTrue(device.wait(Until.hasObject(By.desc("服务地址")), 5000))
        device.takeScreenshot(java.io.File(context.filesDir, "ui-server.png"))
        assertNull(device.findObject(By.desc("密码")))
        assertNull(device.findObject(By.desc("验证码")))
        device.pressBack(); device.pressBack()
        assertTrue(device.wait(Until.hasObject(By.desc("邮箱")), 5000))
        assertEquals("flow-test@example.invalid", device.findObject(By.desc("邮箱")).text)
        device.findObject(By.text("验证邮箱")).click()
        assertTrue(device.wait(Until.hasObject(By.desc("验证码")), 5000))
        device.takeScreenshot(java.io.File(context.filesDir, "ui-verification.png"))
        assertNull(device.findObject(By.desc("密码")))
        assertNull(device.findObject(By.desc("服务地址")))
        assertEquals("flow-test@example.invalid", device.findObject(By.desc("邮箱")).text)
        device.findObject(By.text("返回登录")).click()
        device.findObject(By.text("注册账号")).click()
        assertTrue(device.wait(Until.hasObject(By.desc("确认密码")), 5000))
        assertNull(device.findObject(By.desc("验证码")))
        assertNull(device.findObject(By.desc("服务地址")))
        device.pressBack()
        device.findObject(By.text("设置")).click()
        device.findObject(By.desc("软件更新")).click()
        assertTrue(device.wait(Until.hasObject(By.text("自动检查更新")), 5000))
        assertNull(device.findObject(By.desc("邮箱")))
        assertNull(device.findObject(By.desc("密码")))
    }
}
