package app.farsail.android

import android.content.Intent
import android.view.ViewGroup
import android.widget.ScrollView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Local synthetic coordinator; UI sharing status does not grant projection. */
@RunWith(AndroidJUnit4::class)
class HomeLayoutTest {
    @Test fun devicesAreHomeAndSharingLivesUnderSettings() {
        org.junit.Assume.assumeTrue("Dedicated emulator only", android.os.Build.MODEL.contains("sdk", true))
        val inst = InstrumentationRegistry.getInstrumentation()
        val context = inst.targetContext
        val app = context.applicationContext as FarSailApp
        val device = UiDevice.getInstance(inst)
        val end = System.currentTimeMillis() + 15000
        while (!app.ready && System.currentTimeMillis() < end) Thread.sleep(100)
        assertTrue(app.ready)
        RemotePreferences.clear(context); app.updates.preferences(false, false)
        runCatching { app.invoke("logout") }
        val coordinator = FakeCoordinator()
        var activity: android.app.Activity? = null
        try {
            app.invoke("server", JSONObject().put("url", "http://127.0.0.1:${coordinator.port}"))
            app.invoke("login", JSONObject().put("email", "layout@example.invalid").put("password", "synthetic-layout-password"))
            app.invoke("bind", JSONObject().put("name", "Owned layout fixture"))
            inst.runOnMainSync { app.state = app.invoke("state") as JSONObject }
            val screen = inst.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            activity = screen
            assertTrue(device.wait(Until.hasObject(By.text("我的设备")), 5000))
            assertTrue(device.wait(Until.hasObject(By.text("刷新设备").enabled(true)), 5000))
            fun homeOnlyHasDevices() {
                assertNull(device.findObject(By.text("共享这台手机")))
                assertNull(device.findObject(By.text("开启屏幕共享")))
                assertNull(device.findObject(By.text("允许同账号设备直接控制")))
                assertNull(device.findObject(By.text("共享期间防止自动熄屏")))
            }
            homeOnlyHasDevices(); device.waitForIdle()
            device.takeScreenshot(java.io.File(context.filesDir, "navigation-home.png"))
            openSharingSettings(device)
            assertNull(device.findObject(By.text("刷新设备")))
            assertTrue(device.wait(Until.hasObject(By.text("开启屏幕共享")), 5000))
            val scroll = screen.findViewById<ViewGroup>(android.R.id.content).getChildAt(0) as ScrollView
            fun safeViewport() {
                inst.runOnMainSync {
                    val bars = ViewCompat.getRootWindowInsets(scroll)!!.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
                    val location = IntArray(2); scroll.getLocationOnScreen(location)
                    assertTrue("Viewport stays below status bar", location[1] + scroll.paddingTop >= bars.top)
                    assertTrue("Viewport stays above navigation bar", location[1] + scroll.height - scroll.paddingBottom <= device.displayHeight - bars.bottom)
                    assertTrue(scroll.clipToPadding)
                }
            }
            safeViewport(); device.takeScreenshot(java.io.File(context.filesDir, "navigation-sharing-off.png"))
            inst.runOnMainSync { CaptureService.phase = "sharing"; CaptureService.running = true; app.observer?.invoke() }
            assertTrue(device.wait(Until.hasObject(By.text("停止共享")), 5000))
            device.waitForIdle(); device.takeScreenshot(java.io.File(context.filesDir, "navigation-sharing-active.png"))
            val scroller = UiScrollable(UiSelector().scrollable(true))
            fun reveal(text: String) {
                if (scroller.exists()) scroller.scrollIntoView(UiSelector().text(text))
                device.waitForIdle(); Thread.sleep(300)
                assertNotNull(device.wait(Until.findObject(By.text(text)), 5000))
            }
            reveal("允许同账号设备直接控制"); safeViewport()
            device.takeScreenshot(java.io.File(context.filesDir, "navigation-sharing-scrolled.png"))
            reveal("打开 App 时恢复上次共享")
            reveal("共享与锁屏说明"); device.findObject(By.text("共享与锁屏说明")).click()
            assertTrue(device.wait(Until.hasObject(By.text("知道了")), 5000))
            assertNotNull(device.findObject(By.textContains("会增加耗电")))
            device.findObject(By.text("知道了")).click()
            assertTrue(device.wait(Until.gone(By.text("知道了")), 5000))
            device.pressBack()
            assertTrue(device.wait(Until.hasObject(By.desc("共享这台手机")), 5000))
            assertNull(device.findObject(By.text("允许同账号设备直接控制")))
            device.takeScreenshot(java.io.File(context.filesDir, "navigation-settings.png"))
            device.pressBack()
            assertTrue(device.wait(Until.hasObject(By.text("我的设备")), 5000))
            homeOnlyHasDevices()
            // Incoming requests remain discoverable without putting host settings on home.
            inst.runOnMainSync {
                app.pending = JSONArray().put(JSONObject().put("id", "layout-request").put("source_device_name", "Owned request fixture").put("permission", "view"))
                app.observer?.invoke()
            }
            val request = device.wait(Until.findObject(By.text("收到 1 个连接请求 · 处理")), 5000)
            assertNotNull(request); request.click()
            assertTrue(device.wait(Until.hasObject(By.text("允许连接")), 5000))
            assertNotNull(device.findObject(By.text("拒绝")))
        } finally {
            inst.runOnMainSync {
                CaptureService.running = false; CaptureService.phase = "off"; app.pending = JSONArray(); activity?.finish()
            }
            RemotePreferences.clear(context); runCatching { app.invoke("logout") }; coordinator.close()
        }
    }
}
