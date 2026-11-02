package app.farsail.android

import android.app.*
import android.content.*
import android.media.projection.MediaProjectionConfig
import android.media.projection.MediaProjectionManager
import android.os.*
import android.provider.Settings
import android.text.InputType
import android.view.View
import android.view.WindowInsets
import android.view.inputmethod.EditorInfo
import android.widget.*
import org.json.JSONObject

class MainActivity : Activity() {
    private enum class Page { LOGIN, REGISTER, VERIFY, SERVER, SETTINGS, UPDATES, HOME }
    private val app get() = application as FarSailApp
    private lateinit var ui: MobileUi
    private lateinit var body: LinearLayout
    private lateinit var notice: TextView
    private var page = Page.LOGIN
    private var navigationEpoch = 0L
    private var rendered: Page? = null
    private val history = java.util.ArrayDeque<Page>()
    private val busyButtons = mutableListOf<Button>()
    private var email = ""
    private var authEmail: EditText? = null
    private var status: TextView? = null
    private var share: Button? = null
    private var control: Switch? = null
    private var requestBox: LinearLayout? = null
    private var lastRequests = ""
    private var boundWhenBuilt = false
    private var updateText: TextView? = null
    private var updateButton: Button? = null
    private var updateProgress: ProgressBar? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ui = MobileUi(this)
        email = savedInstanceState?.getString("email") ?: ""
        page = savedInstanceState?.getString("page")?.let { runCatching { Page.valueOf(it) }.getOrNull() } ?: Page.LOGIN
        val scroll = ScrollView(this).apply { setBackgroundColor(ui.background); isFillViewport = true }
        body = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(ui.dp(24), ui.dp(16), ui.dp(24), ui.dp(24)) }
        scroll.addView(body); setContentView(scroll)
        scroll.setOnApplyWindowInsetsListener { _, insets ->
            val top: Int; val bottom: Int
            if (Build.VERSION.SDK_INT >= 30) { val bars = insets.getInsets(WindowInsets.Type.systemBars()); top = bars.top; bottom = bars.bottom }
            else { @Suppress("DEPRECATION") val t = insets.systemWindowInsetTop; @Suppress("DEPRECATION") val b = insets.systemWindowInsetBottom; top = t; bottom = b }
            body.setPadding(ui.dp(24), top + ui.dp(12), ui.dp(24), bottom + ui.dp(24)); insets
        }
        draw()
    }
    override fun onResume() { super.onResume(); app.observer = { draw() }; draw(); app.updates.maybeCheck() }
    override fun onPause() { app.observer = null; super.onPause() }
    override fun onSaveInstanceState(outState: Bundle) { rememberEmail(); outState.putString("email", email); outState.putString("page", page.name); super.onSaveInstanceState(outState) }
    @Deprecated("Framework back callback") override fun onBackPressed() { if (!back()) super.onBackPressed() }
    private fun rememberEmail() { authEmail?.let { email = it.text.toString().trim() } }
    private fun go(next: Page, replace: Boolean = false) {
        rememberEmail(); authEmail = null
        if (!replace && next != page) history.addLast(page)
        navigationEpoch++; page = next; rendered = null; app.message = ""; draw()
    }
    private fun back(): Boolean {
        if (history.isEmpty()) return false
        val next = history.removeLast(); go(next, true); return true
    }
    private fun perform(op: String, args: JSONObject = JSONObject(), success: String = "", after: (() -> Unit)? = null) {
        val owner = page; val token = navigationEpoch
        app.act(op, args, success, owner.name, { !isFinishing && !isDestroyed && page == owner && navigationEpoch == token }, after)
    }
    private fun say(text: String) { app.messageScope = page.name; app.message = text }
    private fun signedIn() = app.state.optBoolean("signedIn")
    private fun hasServer() = app.state.optString("server").let { it.isNotBlank() && it != "http://127.0.0.1:8787" }
    private fun bound() = app.state.optString("deviceId").let { it.isNotBlank() && it != "null" }
    private fun primary(parent: LinearLayout, title: String, action: () -> Unit) = ui.button(parent, title, action = action).also { busyButtons.add(it); it.isEnabled = app.ready && !app.busy }
    private fun draw() {
        if (signedIn() && page in listOf(Page.LOGIN, Page.REGISTER, Page.VERIFY)) { page = Page.HOME; rendered = null; history.clear() }
        if (!signedIn() && page == Page.HOME) { page = Page.LOGIN; rendered = null; history.clear() }
        if (page == Page.HOME && boundWhenBuilt != bound()) rendered = null
        if (rendered != page) {
            rendered = page; body.removeAllViews(); busyButtons.clear(); authEmail = null
            status = null; share = null; control = null; requestBox = null; updateText = null; updateButton = null; updateProgress = null
            header()
            when (page) {
                Page.LOGIN -> login()
                Page.REGISTER -> register()
                Page.VERIFY -> verify()
                Page.SERVER -> server()
                Page.SETTINGS -> settings()
                Page.UPDATES -> updates()
                Page.HOME -> home()
            }
        }
        busyButtons.forEach { it.isEnabled = app.ready && !app.busy; it.alpha = if (it.isEnabled) 1f else .55f }
        notice.text = if (app.busy && app.busyScope == page.name) "正在处理…" else if (app.messageScope == page.name) app.message else ""
        notice.visibility = if (notice.text.isNullOrEmpty()) View.GONE else View.VISIBLE
        if (page == Page.HOME) updateHome()
        if (page == Page.UPDATES) updateUpdates()
    }
    private fun header() {
        val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = android.view.Gravity.CENTER_VERTICAL }
        body.addView(row, LinearLayout.LayoutParams(-1, ui.dp(56)))
        if (page != Page.LOGIN && page != Page.HOME) {
            ui.link(row, "返回") { if (!back()) go(if (signedIn()) Page.HOME else Page.LOGIN, true) }.layoutParams = LinearLayout.LayoutParams(ui.dp(64), -1)
        } else {
            row.addView(ImageView(this).apply { setImageResource(R.drawable.farsail); importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO }, LinearLayout.LayoutParams(ui.dp(30), ui.dp(30)))
        }
        val brand = TextView(this).apply { text = "FarSail 遥舟"; textSize = 18f; setTextColor(ui.ink); setPadding(ui.dp(10), 0, 0, 0) }
        row.addView(brand, LinearLayout.LayoutParams(0, -2, 1f))
        if (page == Page.LOGIN || page == Page.HOME) ui.link(row, "设置") { go(Page.SETTINGS) }.layoutParams = LinearLayout.LayoutParams(ui.dp(64), -1)
        ui.gap(body, 24)
        notice = ui.text(body, "", 14f, secondary = true).apply { setPadding(0, 0, 0, ui.dp(16)) }
    }
    private fun title(text: String, detail: String) { ui.text(body, text, 28f, bold = true); ui.gap(body, 8); ui.text(body, detail, 14f, secondary = true); ui.gap(body, 24) }
    private fun emailField(parent: LinearLayout): EditText = ui.field(parent, "邮箱", "name@example.com", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS, email).also { authEmail = it; it.setAutofillHints(View.AUTOFILL_HINT_EMAIL_ADDRESS) }
    private fun passwordField(parent: LinearLayout, label: String = "密码") = ui.field(parent, label, "请输入密码", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD).apply { isSaveEnabled = false; setAutofillHints(View.AUTOFILL_HINT_PASSWORD) }
    private fun login() {
        title("登录", "登录后，让电脑连接这台手机。")
        val card = ui.card(body); val mail = emailField(card); val password = passwordField(card)
        val submit = primary(card, "登录") {
            rememberEmail()
            when {
                email.isBlank() || password.text.isNullOrEmpty() -> { say("请输入邮箱和密码。"); draw() }
                !hasServer() -> go(Page.SERVER)
                else -> perform("login", JSONObject().put("email", email).put("password", password.text.toString()), "") { password.text.clear() }
            }
        }
        password.imeOptions = EditorInfo.IME_ACTION_DONE; password.setOnEditorActionListener { _, action, _ -> if (action == EditorInfo.IME_ACTION_DONE) { submit.performClick(); true } else false }
        ui.link(card, "注册账号") { go(Page.REGISTER) }
        ui.gap(body, 12); ui.link(body, "验证邮箱") { go(Page.VERIFY) }
    }
    private fun register() {
        title("注册账号", "注册完成后，前往邮箱验证。")
        val card = ui.card(body); emailField(card); val password = passwordField(card).apply { hint = "至少 12 位" }; val confirm = passwordField(card, "确认密码")
        primary(card, "创建账号") {
            rememberEmail()
            if (email.isBlank() || password.text.length < 12 || password.text.toString() != confirm.text.toString()) { say("请填写邮箱和至少 12 位密码，并确认两次密码一致。"); draw(); return@primary }
            if (!hasServer()) { go(Page.SERVER); return@primary }
            perform("register", JSONObject().put("email", email).put("password", password.text.toString()), "") { password.text.clear(); confirm.text.clear(); go(Page.VERIFY, true) }
        }
        ui.link(body, "已有账号，返回登录") { history.clear(); go(Page.LOGIN, true) }
    }
    private fun verify() {
        title("验证邮箱", "填写邮箱收到的六位验证码。")
        val card = ui.card(body); emailField(card)
        val code = ui.field(card, "验证码", "6 位数字", InputType.TYPE_CLASS_NUMBER).apply { filters = arrayOf(android.text.InputFilter.LengthFilter(6)); letterSpacing = .2f; isSaveEnabled = false }
        primary(card, "验证邮箱") {
            rememberEmail()
            if (email.isBlank() || code.text.length != 6) { say("请填写邮箱和六位验证码。"); draw(); return@primary }
            if (!hasServer()) { go(Page.SERVER); return@primary }
            perform("verify", JSONObject().put("email", email).put("token", code.text.toString()), "") { code.text.clear(); history.clear(); go(Page.LOGIN, true); say("邮箱验证成功，请登录。"); draw() }
        }
        ui.link(card, "重新发送验证码") {
            rememberEmail()
            if (email.isBlank()) { say("请先填写邮箱。"); draw() }
            else if (!hasServer()) go(Page.SERVER)
            else perform("resend", JSONObject().put("email", email), "验证码已发送，请查收邮箱。")
        }
        ui.link(body, "返回登录") { history.clear(); go(Page.LOGIN, true) }
    }
    private fun server() {
        title("服务器设置", "与电脑端使用同一个服务地址。")
        val card = ui.card(body)
        val address = ui.field(card, "服务地址", "https://你的服务器", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI, app.state.optString("server").takeUnless { it == "http://127.0.0.1:8787" } ?: "")
        if (signedIn()) {
            address.isEnabled = false
            ui.button(card, "退出登录后修改", false) {
                AlertDialog.Builder(this).setTitle("更换服务器").setMessage("需要先停止共享并退出当前账号。")
                    .setPositiveButton("退出并修改") { _, _ -> stop(); perform("logout") { go(Page.SERVER, true) } }.setNegativeButton("取消", null).show()
            }
        } else primary(card, "保存设置") {
            perform("server", JSONObject().put("url", address.text.toString().trim()), "服务地址已保存。") { if (!back()) go(Page.LOGIN, true) }
        }
    }
    private fun settings() {
        title("设置", "连接与应用选项")
        ui.row(body, "服务器设置", if (hasServer()) app.state.optString("server") else "尚未设置") { go(Page.SERVER) }
        ui.row(body, "软件更新", "当前版本 ${app.updates.currentVersion}") { go(Page.UPDATES) }
        ui.row(body, "后台运行", "管理手机的后台与电池设置") {
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.parse("package:$packageName")))
        }
        if (signedIn()) ui.link(body, "退出登录") { stop(); perform("logout", success = "") { history.clear(); go(Page.LOGIN, true) } }
    }
    private fun updates() {
        title("软件更新", "当前版本 ${app.updates.currentVersion}")
        val card = ui.card(body); updateText = ui.text(card, "", 15f)
        ui.gap(card, 16); updateProgress = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply { max = 100; card.addView(this, LinearLayout.LayoutParams(-1, ui.dp(6))) }
        ui.gap(card, 20); updateButton = ui.button(card, "检查更新") {
            when (app.updates.state.phase) { "available" -> app.updates.download(); "ready" -> app.updates.install(this); else -> app.updates.check() }
        }
        ui.gap(body, 20)
        body.addView(Switch(this).apply { text = "自动检查更新"; isChecked = app.updates.autoCheck; setPadding(0, ui.dp(12), 0, ui.dp(12)); setOnCheckedChangeListener { _, v -> app.updates.preferences(v, app.updates.autoDownload) } })
        body.addView(Switch(this).apply { text = "自动下载更新"; isChecked = app.updates.autoDownload; setPadding(0, ui.dp(12), 0, ui.dp(12)); setOnCheckedChangeListener { _, v -> app.updates.preferences(app.updates.autoCheck, v) } })
        ui.gap(body, 10); ui.text(body, "安装前请结束屏幕共享。安装由手机系统确认。", 13f, secondary = true)
    }
    private fun updateUpdates() {
        val s = app.updates.state
        updateText?.text = listOf(if (s.version.isNotEmpty()) "新版本 ${s.version}" else "", s.message).filter { it.isNotEmpty() }.joinToString("\n\n")
        updateProgress?.visibility = if (s.phase == "downloading") View.VISIBLE else View.GONE; updateProgress?.progress = s.progress
        updateButton?.text = when (s.phase) { "checking" -> "正在检查…"; "downloading" -> "下载中 ${s.progress}%"; "available" -> "下载更新"; "ready" -> "安装更新"; else -> "检查更新" }
        updateButton?.isEnabled = s.phase !in listOf("checking", "downloading")
    }
    private fun home() {
        boundWhenBuilt = bound(); lastRequests = ""
        title("这台手机", "在电脑端登录同一账号，选择这台手机连接。")
        val card = ui.card(body)
        if (!bound()) {
            val name = ui.field(card, "手机名称", value = "${Build.MANUFACTURER} ${Build.MODEL}")
            primary(card, "添加这台手机") { perform("bind", JSONObject().put("name", name.text.toString().trim()), "") }
        } else {
            status = ui.text(card, "", 19f, bold = true); ui.gap(card, 8); ui.text(card, "连接请求需要在手机上确认。", 13f, secondary = true); ui.gap(card, 20)
            share = ui.button(card, "开启屏幕共享") { if (CaptureService.running) { stop(); draw() } else consent() }
            ui.gap(body, 16); val input = ui.card(body); ui.text(input, "远程控制", 18f, bold = true); ui.gap(input, 8)
            control = Switch(this).apply { text = "允许已批准的电脑控制"; setPadding(0, ui.dp(12), 0, ui.dp(12)); input.addView(this) }
            ui.link(input, "打开辅助功能设置") {
                AlertDialog.Builder(this).setTitle("允许远程控制").setMessage("开启 FarSail 辅助功能后，还需打开控制开关、确认屏幕共享，并逐次批准连接。")
                    .setPositiveButton("前往设置") { _, _ -> startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) }.setNegativeButton("取消", null).show()
            }
        }
        ui.gap(body, 16); requestBox = ui.column(body)
    }
    private fun updateHome() {
        status?.text = when { CaptureService.running && app.state.optBoolean("sharing") -> "屏幕共享中"; CaptureService.running -> "正在建立连接…"; else -> "屏幕共享已关闭" }
        share?.text = if (CaptureService.running) "停止共享" else "开启屏幕共享"
        share?.isEnabled = app.ready && (CaptureService.running || !app.busy)
        control?.apply { setOnCheckedChangeListener(null); isEnabled = ControlService.instance != null; isChecked = app.state.optBoolean("control") && isEnabled
            setOnCheckedChangeListener { _, checked -> NativeBridge.setControl(checked); ControlService.cancel(); app.io.execute { try { app.refresh() } catch (_: Exception) { } } }
        }
        val snapshot = app.pending.toString() + app.state.optJSONObject("session").toString()
        if (snapshot == lastRequests) return
        lastRequests = snapshot; val box = requestBox ?: return; busyButtons.removeAll { it.parent === box }; box.removeAllViews()
        app.state.optJSONObject("session")?.let { ui.text(box, "已连接 · ${if (it.optString("permission") == "control") "可控制" else "仅查看"}", 18f, bold = true); ui.text(box, "核对码：${it.optString("code", "—")}", 14f, secondary = true); ui.gap(box, 16) }
        for (i in 0 until app.pending.length()) {
            val r = app.pending.getJSONObject(i); ui.text(box, r.optString("source_device_name", "电脑"), 18f, bold = true); ui.text(box, "${r.optString("requester_email")} 请求${if (r.optString("permission") == "control") "控制手机" else "查看屏幕"}", 14f, secondary = true); ui.gap(box, 16)
            primary(box, "允许连接") { perform("decide", JSONObject().put("id", r.getString("id")).put("approve", true), "已批准连接") }
            ui.button(box, "拒绝", false) { perform("decide", JSONObject().put("id", r.getString("id")).put("approve", false), "已拒绝连接") }
        }
    }
    private fun stop() { NativeBridge.stopLocal(); ControlService.cancel(); CaptureService.stop(this) }
    private fun consent() {
        val manager = getSystemService(MediaProjectionManager::class.java)
        val intent = if (Build.VERSION.SDK_INT >= 34) manager.createScreenCaptureIntent(MediaProjectionConfig.createConfigForDefaultDisplay()) else manager.createScreenCaptureIntent()
        @Suppress("DEPRECATION") startActivityForResult(intent, 42)
    }
    @Deprecated("Framework result callback") override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == 42 && resultCode == RESULT_OK && data != null) startForegroundService(Intent(this, CaptureService::class.java).putExtra("result", resultCode).putExtra("data", data))
        else if (requestCode == 42) { say("已取消屏幕共享"); draw() }
    }
}
