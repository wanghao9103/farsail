package app.farsail.android

import android.app.*
import android.content.*
import android.graphics.Color
import android.media.projection.MediaProjectionConfig
import android.media.projection.MediaProjectionManager
import android.os.*
import android.provider.Settings
import android.text.InputType
import android.view.View
import android.widget.*
import org.json.JSONObject

class MainActivity : Activity() {
    private val app get() = application as FarSailApp
    private lateinit var body: LinearLayout
    private lateinit var notice: TextView
    private lateinit var status: TextView
    private lateinit var requests: LinearLayout
    private lateinit var control: Switch
    private lateinit var share: Button
    private val buttons = mutableListOf<Button>()
    private var mode = ""
    private var registration = false
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val scroll = ScrollView(this).apply { setBackgroundColor(Color.rgb(244, 246, 243)); isFillViewport = true }
        body = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(dp(24), dp(34), dp(24), dp(32)) }
        scroll.addView(body); setContentView(scroll)
        draw()
    }
    override fun onResume() { super.onResume(); app.observer = { draw() }; draw() }
    override fun onPause() { app.observer = null; super.onPause() }
    private fun dp(n: Int) = (n * resources.displayMetrics.density).toInt()
    private fun label(text: String, size: Float = 15f, parent: LinearLayout = body): TextView = TextView(this).also {
        it.text = text; it.textSize = size; it.setTextColor(Color.rgb(36, 51, 44)); it.setPadding(0, dp(8), 0, dp(8)); parent.addView(it)
    }
    private fun field(hint: String, password: Boolean = false, value: String = ""): EditText = EditText(this).also {
        it.hint = hint; it.setText(value); it.isSingleLine = true; it.textSize = 16f
        it.inputType = if (password) InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD else InputType.TYPE_CLASS_TEXT
        body.addView(it, LinearLayout.LayoutParams(-1, dp(56)))
    }
    private fun button(text: String, parent: LinearLayout = body, action: () -> Unit): Button = Button(this).also {
        it.text = text; it.isAllCaps = false; it.setOnClickListener { action() }
        parent.addView(it, LinearLayout.LayoutParams(-1, dp(52)).apply { topMargin = dp(8) }); buttons.add(it)
    }
    private fun draw() {
        val next = if (app.state.optBoolean("signedIn")) "home" else if (registration) "register" else "login"
        if (mode != next) {
            mode = next; body.removeAllViews(); buttons.clear()
            label("FarSail 遥舟", 30f)
            label("让电脑连接这台手机", 17f)
            notice = label("")
            if (mode == "home") home() else auth()
        }
        notice.text = if (app.busy) "正在处理…" else app.message
        buttons.forEach { it.isEnabled = app.ready && !app.busy }
        if (mode == "home") {
            status.text = when {
                CaptureService.running && app.state.optBoolean("sharing") -> "屏幕共享已开启 · 可在后台继续运行"
                CaptureService.running -> "正在建立连接…"
                else -> "屏幕共享已关闭"
            }
            share.text = if (CaptureService.running) "停止共享" else "开启屏幕共享"
            share.isEnabled = app.ready && (CaptureService.running || !app.busy)
            control.isEnabled = ControlService.instance != null
            control.setOnCheckedChangeListener(null)
            control.isChecked = app.state.optBoolean("control") && ControlService.instance != null
            control.setOnCheckedChangeListener { _, checked -> NativeBridge.setControl(checked); ControlService.cancel(); app.io.execute { try { app.refresh() } catch (e: Exception) { app.message = "状态刷新失败，请检查网络。" } } }
            // Request controls are rebuilt, so drop references to their old views.
            buttons.removeAll { it.parent === requests }
            requests.removeAllViews()
            val session = app.state.optJSONObject("session")
            if (session != null) {
                label("已连接 · ${if (session.optString("permission") == "control") "可控制" else "仅查看"}", 19f, requests)
                label("核对码：${session.optString("code", "—")}\n连接：${session.optString("path")}", 15f, requests)
            } else if (CaptureService.running) label("等待电脑连接", 19f, requests)
            for (i in 0 until app.pending.length()) {
                val row = app.pending.getJSONObject(i)
                label(row.optString("source_device_name", "电脑"), 19f, requests)
                label("${row.optString("requester_email")} 请求${if (row.optString("permission") == "control") "控制手机" else "查看屏幕"}", 15f, requests)
                button("允许连接", requests) { app.act("decide", JSONObject().put("id", row.getString("id")).put("approve", true), "已批准连接") }.isEnabled = !app.busy
                button("拒绝", requests) { app.act("decide", JSONObject().put("id", row.getString("id")).put("approve", false), "已拒绝连接") }.isEnabled = !app.busy
            }
        }
    }
    private fun auth() {
        val server = field("服务地址，例如 https://你的服务器", value = app.state.optString("server").takeUnless { it.isBlank() || it.contains("127.0.0.1") } ?: "")
        server.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
        button("保存服务地址") { app.act("server", JSONObject().put("url", server.text.toString().trim()), "服务地址已保存") }
        val email = field("邮箱")
        email.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS
        val password = field("密码", true)
        button(if (registration) "注册账号" else "登录") {
            app.act(if (registration) "register" else "login", JSONObject().put("email", email.text.toString().trim()).put("password", password.text.toString()), if (registration) "请查收邮箱验证码。" else "已登录") { password.text.clear() }
        }
        val code = field("邮箱验证码（未验证时填写）")
        code.inputType = InputType.TYPE_CLASS_NUMBER
        button("验证邮箱") { app.act("verify", JSONObject().put("email", email.text.toString().trim()).put("token", code.text.toString().trim()), "邮箱验证成功，请登录") }
        button("重新发送验证码") { app.act("resend", JSONObject().put("email", email.text.toString().trim()), "验证码已发送") }
        button(if (registration) "已有账号，去登录" else "没有账号，去注册") { registration = !registration; mode = ""; draw() }
    }
    private fun home() {
        status = label("", 21f)
        val device = app.state.optString("deviceId").takeUnless { it == "null" || it.isBlank() }
        if (device == null) {
            val name = field("手机名称", value = "${Build.MANUFACTURER} ${Build.MODEL}")
            button("添加这台手机") { app.act("bind", JSONObject().put("name", name.text.toString().trim()), "手机已添加") { mode = ""; draw() } }
        } else {
            label("手机已添加。在电脑端登录同一账号，选择这台手机连接。")
            label("1. 仅查看屏幕可直接开启共享\n2. 需要点击和输入时，开启辅助功能并允许控制\n3. 每次连接都在此确认")
            button("打开辅助功能设置") {
                AlertDialog.Builder(this).setTitle("允许远程控制")
                    .setMessage("开启 FarSail 辅助功能后，仍需打开下方控制开关、确认屏幕共享，并逐次批准连接。对方才能点击、滑动及向普通输入框输入文字。")
                    .setPositiveButton("前往设置") { _, _ -> startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) }.setNegativeButton("取消", null).show()
            }
            control = Switch(this).apply { text = "允许已批准的电脑控制手机"; setPadding(0, dp(12), 0, dp(12)) }; body.addView(control)
            share = button("开启屏幕共享") {
                if (CaptureService.running) { NativeBridge.stopLocal(); ControlService.cancel(); CaptureService.stop(this) }
                else consent()
            }
            button("后台运行设置") {
                AlertDialog.Builder(this).setTitle("后台继续共享")
                    .setMessage("切换应用或关闭这个页面后可继续共享。若手机系统频繁结束共享，可在应用设置的电池选项中允许后台运行。锁屏、系统收回录屏权限或强行停止后，需要重新开启共享。新安卓默认不申请通知权限；系统仍会显示运行状态。旧安卓保留静默通知。")
                    .setPositiveButton("应用设置") { _, _ -> startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.parse("package:$packageName"))) }.setNegativeButton("知道了", null).show()
            }
        }
        requests = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(0, dp(16), 0, dp(16)) }; body.addView(requests)
        // Keep shared render fields available while the phone is not bound yet.
        if (device == null) { control = Switch(this); share = Button(this) }
        button("退出登录") { NativeBridge.stopLocal(); ControlService.cancel(); CaptureService.stop(this); app.act("logout", success = "已退出登录") }
    }
    private fun consent() {
        val manager = getSystemService(MediaProjectionManager::class.java)
        val intent = if (Build.VERSION.SDK_INT >= 34) manager.createScreenCaptureIntent(MediaProjectionConfig.createConfigForDefaultDisplay()) else manager.createScreenCaptureIntent()
        @Suppress("DEPRECATION")
        startActivityForResult(intent, 42)
    }
    @Deprecated("Framework result callback")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == 42 && resultCode == RESULT_OK && data != null) {
            startForegroundService(Intent(this, CaptureService::class.java).putExtra("result", resultCode).putExtra("data", data))
        } else if (requestCode == 42) { app.message = "已取消屏幕共享"; draw() }
    }
}
