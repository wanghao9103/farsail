package app.farsail.android

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.view.View
import android.view.WindowInsets
import android.widget.*
import org.json.JSONArray
import java.util.concurrent.Executors

/** Controller discovery is separate from local sharing and its system permissions. */
class DeviceListActivity : Activity() {
    private val app get() = application as FarSailApp
    private lateinit var ui: MobileUi
    private lateinit var list: LinearLayout
    private lateinit var message: TextView
    private lateinit var refresh: Button
    private val worker = Executors.newSingleThreadExecutor()
    private var generation = 0
    private var foreground = false
    private var loading = false
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState); ui = MobileUi(this)
        val scroll = ScrollView(this).apply { setBackgroundColor(ui.background); isFillViewport = true }
        val body = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(ui.dp(20), ui.dp(20), ui.dp(20), ui.dp(20)) }
        scroll.addView(body); setContentView(scroll)
        body.setOnApplyWindowInsetsListener { _, insets ->
            if (android.os.Build.VERSION.SDK_INT >= 30) {
                val bars = insets.getInsets(WindowInsets.Type.systemBars()); body.setPadding(ui.dp(20), bars.top + ui.dp(12), ui.dp(20), bars.bottom + ui.dp(20))
            }; insets
        }
        ui.link(body, "返回") { finish() }
        ui.text(body, "我的设备", 28f, bold = true); ui.gap(body, 8)
        ui.text(body, "选择要控制的电脑或安卓手机。被控设备需在线并开启共享，控制权限沿用对方的授权设置。", 14f, secondary = true)
        ui.gap(body, 20); refresh = ui.button(body, "刷新设备") { load() }
        message = ui.text(body, "", 14f, secondary = true); ui.gap(body, 12)
        list = ui.column(body)
    }
    override fun onResume() { super.onResume(); foreground = true; load() }
    override fun onPause() { foreground = false; generation++; loading = false; super.onPause() }
    override fun onDestroy() { generation++; worker.shutdownNow(); super.onDestroy() }
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
                app.main.post { if (foreground && ticket == generation && !isFinishing) { loading = false; refresh.isEnabled = true; show(devices, own) } }
            } catch (e: Exception) { app.main.post { if (foreground && ticket == generation && !isFinishing) { loading = false; refresh.isEnabled = true; message.text = app.friendly(e.message ?: "设备读取失败") } } }
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
                    startActivity(Intent(this, ViewerActivity::class.java).putExtra("target", device.getString("id")).putExtra("name", device.optString("name")).putExtra("permission", permission))
                }.apply { isEnabled = available; alpha = if (available) 1f else .45f }
            }
            ui.gap(list, 14)
        }
        message.text = if (count == 0) "还没有其他设备。先在电脑或另一台手机登录同一账号并开启共享。" else "共 $count 台其他设备"
    }
}
