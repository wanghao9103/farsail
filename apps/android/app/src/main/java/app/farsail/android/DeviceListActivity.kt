package app.farsail.android

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.widget.*
import org.json.JSONArray
import java.util.concurrent.Executors

/** Controller discovery is separate from local sharing and its system permissions. */
class DeviceListActivity : Activity() {
    private lateinit var devices: DeviceListContent
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val ui = MobileUi(this); val body = ui.screen()
        ui.link(body, "返回") { finish() }
        ui.text(body, "我的设备", 28f, bold = true); ui.gap(body, 16)
        devices = DeviceListContent(this, ui, body)
    }
    override fun onResume() { super.onResume(); devices.resume() }
    override fun onPause() { devices.pause(); super.onPause() }
    override fun onDestroy() { devices.close(); super.onDestroy() }
}

/** The home screen and standalone discovery screen share one device list. */
class DeviceListContent(private val activity: Activity, private val ui: MobileUi, body: LinearLayout) {
    private val app get() = activity.application as FarSailApp
    private lateinit var list: LinearLayout
    private lateinit var message: TextView
    private lateinit var refresh: Button
    private val worker = Executors.newSingleThreadExecutor()
    private var generation = 0
    private var foreground = false
    private var loading = false
    init {
        refresh = ui.button(body, "刷新设备", false) { load() }
        message = ui.text(body, "", 14f, secondary = true); ui.gap(body, 12)
        list = ui.column(body)
    }
    fun resume() { foreground = true; load() }
    fun pause() { foreground = false; generation++; loading = false }
    fun close() { pause(); worker.shutdownNow() }
    private fun load() {
        if (loading || !foreground) return
        if (!app.ready) { message.text = "正在初始化，请稍后刷新。"; return }
        loading = true; refresh.isEnabled = false; message.text = "正在读取设备…"
        val ticket = ++generation
        worker.execute {
            try {
                val state = app.invoke("state") as org.json.JSONObject
                check(state.optBoolean("signedIn")) { "请返回首页登录。" }
                val own = state.optString("deviceId"); check(own.isNotBlank() && own != "null") { "请返回首页添加这台手机。" }
                val devices = app.invoke("devices") as JSONArray
                app.main.post { if (foreground && ticket == generation && !activity.isFinishing && !activity.isDestroyed) { loading = false; refresh.isEnabled = true; show(devices, own) } }
            } catch (e: Exception) { app.main.post { if (foreground && ticket == generation && !activity.isFinishing && !activity.isDestroyed) { loading = false; refresh.isEnabled = true; message.text = app.friendly(e.message ?: "设备读取失败") } } }
        }
    }
    private fun show(devices: JSONArray, own: String) {
        list.removeAllViews(); var count = 0
        for (i in 0 until devices.length()) {
            val device = devices.getJSONObject(i); if (device.optString("id") == own) continue
            count++
            val card = ui.card(list); ui.text(card, device.optString("name", "未命名设备"), 19f, bold = true); ui.gap(card, 6)
            val platform = when (device.optString("platform").lowercase()) { "android" -> "安卓手机"; "windows" -> "Windows 电脑"; "linux" -> "Linux 电脑"; else -> "电脑" }
            val reason = when { !device.optBoolean("enabled") -> "设备已停用"; !device.optBoolean("online") -> "离线"; !device.optBoolean("can_host") -> "未开启共享"; else -> "在线 · 可连接" }
            val available = device.optBoolean("enabled") && device.optBoolean("online") && device.optBoolean("can_host")
            ui.text(card, "$platform · $reason", 14f, secondary = true); ui.gap(card, 16)
            for ((label, permission) in listOf("控制设备" to "control", "仅查看" to "view")) {
                ui.button(card, label, permission == "control") {
                    activity.startActivity(Intent(activity, ViewerActivity::class.java).putExtra("target", device.getString("id")).putExtra("name", device.optString("name")).putExtra("permission", permission))
                }.apply { isEnabled = available; alpha = if (available) 1f else .45f }
            }
            ui.gap(list, 14)
        }
        message.text = if (count == 0) "还没有其他设备。先在电脑或另一台手机登录同一账号并开启共享。" else "共 $count 台其他设备"
    }
}
