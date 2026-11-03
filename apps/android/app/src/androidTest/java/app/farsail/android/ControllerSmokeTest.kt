package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Real JNI/QUIC against the explicitly supplied loopback synthetic peer only. */
@RunWith(AndroidJUnit4::class)
class ControllerSmokeTest {
    private val inst get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = inst.targetContext
    private val app get() = context.applicationContext as FarSailApp
    private val device get() = UiDevice.getInstance(inst)
    private lateinit var url: String
    private fun await(test: () -> Boolean) {
        val until = System.currentTimeMillis() + 30000
        while (System.currentTimeMillis() < until) { if (test()) return; Thread.sleep(100) }
        assertTrue("controller condition timed out", test())
    }
    private fun report(): List<JSONObject> {
        val connection = java.net.URL("$url/fixture/report").openConnection() as java.net.HttpURLConnection
        connection.connectTimeout = 3000; connection.readTimeout = 3000
        return try { val rows = JSONObject(connection.inputStream.bufferedReader().use { it.readText() }).getJSONArray("requests"); (0 until rows.length()).map { rows.getJSONObject(it) } }
        finally { connection.disconnect() }
    }
    private fun inputs(phone: Boolean, permission: String = "control") = report().filter { it.optBoolean("phone") == phone && it.optString("permission") == permission }.flatMap { row ->
        val list = row.getJSONArray("inputs"); (0 until list.length()).map { list.getJSONObject(it) }
    }
    private fun text(label: String): UiObject2 {
        var node = device.findObject(By.text(label))
        if (node == null || node.visibleBounds.isEmpty) {
            val scroller = UiScrollable(UiSelector().scrollable(true))
            if (device.hasObject(By.desc("远程画面"))) scroller.setAsHorizontalList()
            if (scroller.exists()) scroller.scrollIntoView(UiSelector().text(label))
            node = device.wait(Until.findObject(By.text(label)), 5000)
        }
        return node ?: error("Missing controller UI: $label")
    }
    private fun choose(name: String, permission: String = "control") {
        assertNotNull("Device list has finished refreshing", device.wait(Until.findObject(By.text("刷新设备").enabled(true)), 10000))
        device.waitForIdle()
        val title = text(name)
        title.parent.findObject(By.text(if (permission == "control") "控制设备" else "仅查看")).click()
        await { device.hasObject(By.textStartsWith("已连接")) && (device.findObject(By.textStartsWith("已连接"))?.text?.contains(if (permission == "control") "可控制" else "仅查看") == true) }
    }
    private fun point(sourceW: Int, sourceH: Int, turns: Int, x: Float, y: Float): Pair<Int,Int> {
        val bounds = device.findObject(By.desc("远程画面")).visibleBounds
        val width = if (turns % 2 == 0) sourceW else sourceH
        val height = if (turns % 2 == 0) sourceH else sourceW
        val scale = minOf(bounds.width().toFloat()/width, bounds.height().toFloat()/height)
        return (bounds.left + (bounds.width()-width*scale)/2 + width*scale*x).toInt() to (bounds.top + (bounds.height()-height*scale)/2 + height*scale*y).toInt()
    }
    @Test fun desktopPhoneViewAndCancellationThroughRealMobileUi() {
        org.junit.Assume.assumeTrue("External controller driver only", InstrumentationRegistry.getArguments().getString("controllerDriver") == "true")
        org.junit.Assume.assumeTrue("Owned emulator only", android.os.Build.MODEL.contains("sdk",true) || android.os.Build.FINGERPRINT.contains("generic"))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        url = InstrumentationRegistry.getArguments().getString("fixtureUrl") ?: error("fixtureUrl required")
        assertTrue(url.matches(Regex("http://127\\.0\\.0\\.1:[0-9]+")))
        device.wakeUp(); device.pressMenu(); await { app.ready }
        try {
            runCatching { app.invoke("logout") }; RemotePreferences.clear(context)
            app.invoke("server",JSONObject().put("url",url))
            app.invoke("login",JSONObject().put("email","fixture@example.invalid").put("password","synthetic-fixture-password"))
            app.invoke("bind",JSONObject().put("name","Owned mobile controller")); app.refresh()
            await { app.state.optBoolean("signedIn") && app.state.optString("deviceId") != "null" }
            context.startActivity(Intent(context,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            text("我的设备"); choose("Synthetic desktop")
            assertFalse("Controller does not start local screen sharing",CaptureService.running)
            assertFalse((app.invoke("state") as JSONObject).optBoolean("sharing"))
            val p = point(640,360,0,.25f,.75f); device.click(p.first,p.second)
            val q = point(640,360,0,.75f,.25f); device.swipe(p.first,p.second,q.first,q.second,15)
            await { inputs(false).count { it.optString("kind") == "button" } >= 4 && inputs(false).any { it.optString("kind") == "move" } }
            text("滚动模式").click(); assertTrue(text("滚动中").isSelected); device.swipe(p.first,p.second,q.first,q.second,15)
            await { inputs(false).any { it.optString("kind") == "wheel" } }
            text("键盘").click(); text("输入文字").click()
            device.wait(Until.findObject(By.clazz(android.widget.EditText::class.java)),5000).text = "mobile fixture"
            text("发送").click(); await { inputs(false).any { it.optString("text") == "mobile fixture" } }
            await { !device.hasObject(By.text("发送文字")) }; device.waitForIdle(); Thread.sleep(300)
            device.takeScreenshot(java.io.File(context.filesDir,"controller-desktop.png"))
            text("退出").click(); choose("Synthetic phone")
            // The phone peer sends one frame only; control must outlive an idle image.
            Thread.sleep(6200)
            assertTrue(device.findObject(By.textStartsWith("已连接")).text.contains("可控制"))
            text("键盘").click()
            assertFalse(device.hasObject(By.text("Ctrl+V")))
            assertFalse(device.hasObject(By.text("Windows 键")))
            text("全选").click()
            await { inputs(true).any { it.optInt("vk") == 65 && it.optBoolean("down") } }
            for (label in listOf("返回","主屏幕","最近任务")) text(label).click()
            await { inputs(true).filter { it.optString("kind") == "key" && it.optBoolean("down") }.map { it.optInt("vk") }.containsAll(listOf(166,172,93)) }
            text("旋转画面").click()
            val rotated = point(320,640,1,.25f,.75f); device.click(rotated.first,rotated.second)
            await { inputs(true).any { it.optString("kind") == "button" && !it.optBoolean("down") && kotlin.math.abs(it.optDouble("x")-.75)<.015 && kotlin.math.abs(it.optDouble("y")-.75)<.015 } }
            device.takeScreenshot(java.io.File(context.filesDir,"controller-phone.png"))
            text("退出").click(); choose("Synthetic phone","view")
            for (label in listOf("返回","主屏幕","最近任务")) assertFalse(text(label).isEnabled)
            text("旋转画面").click(); device.findObject(By.desc("远程画面")).click(); Thread.sleep(500)
            assertTrue("View grant never sends device input",inputs(true,"view").isEmpty())
            device.pressHome(); await { report().none { it.optBoolean("open") } }
            context.startActivity(Intent(context,DeviceListActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            val pending = text("Pending phone"); pending.parent.findObject(By.text("控制设备")).click()
            await { device.hasObject(By.textStartsWith("等待对方批准")) }
            text("取消连接").click(); await { report().any { !it.optBoolean("approved") && it.optBoolean("revoked") } }
            java.io.File(context.filesDir,"controller-result.json").writeText(JSONObject().put("desktopControl",true).put("phoneControl",true).put("staticFrameControl",true).put("viewDeniedInput",true).put("rotatedTouch",true).put("backgroundClosed",true).put("pendingCancelled",true).put("localSharingStarted",false).toString())
        } catch (e: Throwable) {
            device.takeScreenshot(java.io.File(context.filesDir,"controller-failure.png"))
            device.dumpWindowHierarchy(java.io.File(context.filesDir,"controller-failure.xml"))
            throw e
        } finally { device.pressHome(); runCatching { app.invoke("logout") }; RemotePreferences.clear(context) }
    }
}
