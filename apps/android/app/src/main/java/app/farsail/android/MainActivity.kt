package app.farsail.android

import android.app.*
import android.content.*
import android.media.projection.MediaProjectionConfig
import android.media.projection.MediaProjectionManager
import android.os.*
import android.provider.Settings
import android.text.InputType
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.*
import org.json.JSONObject

class MainActivity : Activity() {
    private enum class Page { LOGIN, REGISTER, VERIFY, SERVER, SETTINGS, SHARING, UPDATES, CONTROL_SETUP, HOME }
    private val app get() = application as FarSailApp
    private lateinit var ui: MobileUi
    private lateinit var body: LinearLayout
    private lateinit var notice: TextView
    private var foreground = false
    private var restoreSharingPending = true
    private var consentOutstanding = false
    private var consentTicket = 0L
    private data class ProjectionResult(val ticket: Long, val data: Intent)
    // System authorization data stays in this Activity only, never preferences or saved state.
    private var pendingProjection: ProjectionResult? = null
    private var page = Page.LOGIN
    private var navigationEpoch = 0L
    private var rendered: Page? = null
    private val history = java.util.ArrayDeque<Page>()
    private val busyButtons = mutableListOf<Button>()
    private var email = ""
    private var authEmail: EditText? = null
    private var status: TextView? = null
    private var sharingDetail: TextView? = null
    private var controlDetail: TextView? = null
    private var pendingControlEnable = false
    private var guideConnected = false
    private var share: Button? = null
    private var control: Switch? = null
    private var requestBox: LinearLayout? = null
    private var lastRequests = ""
    private var boundWhenBuilt = false
    private var updateText: TextView? = null
    private var updateButton: Button? = null
    private var updateProgress: ProgressBar? = null
    private var devices: DeviceListContent? = null
    private var incoming: Button? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ui = MobileUi(this)
        email = savedInstanceState?.getString("email") ?: ""
        pendingControlEnable = savedInstanceState?.getBoolean("pendingControlEnable") ?: false
        consentOutstanding = savedInstanceState?.getBoolean("consentOutstanding") ?: false
        consentTicket = savedInstanceState?.getLong("consentTicket") ?: 0
        page = savedInstanceState?.getString("page")?.let { runCatching { Page.valueOf(it) }.getOrNull() } ?: Page.LOGIN
        body = ui.screen()
        draw()
    }
    override fun onResume() {
        super.onResume(); foreground = true
        if (!consentOutstanding && CaptureService.phase !in listOf("consent", "starting")) restoreSharingPending = true
        app.observer = { draw() }; app.restoreControl(); draw(); devices?.resume(); app.updates.maybeCheck()
    }
    override fun onPause() { foreground = false; devices?.pause(); app.observer = null; super.onPause() }
    override fun onSaveInstanceState(outState: Bundle) { rememberEmail(); outState.putString("email", email); outState.putString("page", page.name); outState.putBoolean("pendingControlEnable", pendingControlEnable); outState.putBoolean("consentOutstanding", consentOutstanding && pendingProjection == null); outState.putLong("consentTicket", consentTicket); super.onSaveInstanceState(outState) }
    override fun onDestroy() {
        devices?.close()
        pendingProjection?.let {
            if (!CaptureService.running && RemotePreferences.abandonConsent(this, it.ticket))
                CaptureService.status(this, "off", "允许共享已记住，返回后重新确认系统录屏授权即可。")
        }
        pendingProjection = null
        super.onDestroy()
    }
    @Deprecated("Framework back callback") override fun onBackPressed() { if (!back()) super.onBackPressed() }
    private fun rememberEmail() { authEmail?.let { email = it.text.toString().trim() } }
    private fun go(next: Page, replace: Boolean = false) {
        if (page == Page.CONTROL_SETUP && next != page) pendingControlEnable = false
        rememberEmail(); authEmail = null
        if (!replace && next != page) history.addLast(page)
        navigationEpoch++; page = next; rendered = null; app.message = ""; draw()
    }
    private fun back(): Boolean {
        if (history.isEmpty()) {
            if (page == Page.SHARING) { go(Page.SETTINGS, true); return true }
            if (page == Page.SETTINGS && signedIn()) { go(Page.HOME, true); return true }
            return false
        }
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
        consumeProjectionResult()
        if (pendingControlEnable && ControlService.instance != null && app.ready && signedIn() && bound()) {
            pendingControlEnable = false
            setControl(true)
            if (history.peekLast() == Page.SHARING) history.removeLast()
            page = Page.SHARING; rendered = null
            say("远程控制已开启，同账号设备无需手机重复确认。")
        }
        if (page == Page.CONTROL_SETUP && guideConnected != (ControlService.instance != null)) rendered = null
        if (signedIn() && page in listOf(Page.LOGIN, Page.REGISTER, Page.VERIFY)) { page = Page.HOME; rendered = null; history.clear() }
        if (!signedIn() && page == Page.HOME) { page = Page.LOGIN; rendered = null; history.clear() }
        if (page in listOf(Page.HOME, Page.SHARING) && boundWhenBuilt != bound()) rendered = null
        if (rendered != page) {
            devices?.close(); devices = null; incoming = null
            rendered = page; body.removeAllViews(); busyButtons.clear(); authEmail = null
            (body.parent as ScrollView).scrollTo(0, 0)
            status = null; sharingDetail = null; controlDetail = null; share = null; control = null; requestBox = null; updateText = null; updateButton = null; updateProgress = null
            header()
            when (page) {
                Page.LOGIN -> login()
                Page.REGISTER -> register()
                Page.VERIFY -> verify()
                Page.SERVER -> server()
                Page.SETTINGS -> settings()
                Page.SHARING -> sharing()
                Page.UPDATES -> updates()
                Page.CONTROL_SETUP -> controlSetup()
                Page.HOME -> home()
            }
        }
        busyButtons.forEach { it.isEnabled = app.ready && !app.busy; it.alpha = if (it.isEnabled) 1f else .55f }
        notice.text = if (!app.ready && app.message.isBlank()) "正在恢复已保存的设置…" else if (app.busy && app.busyScope == page.name) "正在处理…" else if (app.messageScope == page.name) app.message else ""
        notice.visibility = if (notice.text.isNullOrEmpty()) View.GONE else View.VISIBLE
        if (page == Page.SHARING) updateSharing()
        incoming?.apply {
            val count = app.pending.length()
            visibility = if (count > 0) View.VISIBLE else View.GONE
            text = "收到 $count 个连接请求 · 处理"
        }
        if (page == Page.UPDATES) updateUpdates()
        maybeRestoreSharing()
    }
    private fun maybeRestoreSharing() {
        if (CaptureService.running) { restoreSharingPending = false; return }
        if (!foreground || !restoreSharingPending || consentOutstanding || page !in listOf(Page.HOME, Page.SHARING) || !app.ready || app.busy || CaptureService.running || CaptureService.phase !in listOf("off", "interrupted") || !RemotePreferences.autoRestore(this) || !RemotePreferences.wantsSharing(this, app.state)) return
        restoreSharingPending = false
        val token = navigationEpoch
        app.main.post {
            if (foreground && !isFinishing && !isDestroyed && token == navigationEpoch && page in listOf(Page.HOME, Page.SHARING) && !consentOutstanding && !CaptureService.running && CaptureService.phase in listOf("off", "interrupted") && RemotePreferences.autoRestore(this) && RemotePreferences.wantsSharing(this, app.state)) consent(true)
        }
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
        ui.gap(body, if (page == Page.HOME) 12 else 24)
        notice = ui.text(body, "", 14f, secondary = true).apply { setPadding(0, 0, 0, ui.dp(16)) }
    }
    private fun title(text: String, detail: String) { ui.text(body, text, 28f, bold = true); ui.gap(body, 8); ui.text(body, detail, 14f, secondary = true); ui.gap(body, 24) }
    private fun emailField(parent: LinearLayout): EditText = ui.field(parent, "邮箱", "name@example.com", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS, email).also { authEmail = it; it.setAutofillHints(View.AUTOFILL_HINT_EMAIL_ADDRESS) }
    private fun passwordField(parent: LinearLayout, label: String = "密码") = ui.field(parent, label, "请输入密码", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD).apply { isSaveEnabled = false; setAutofillHints(View.AUTOFILL_HINT_PASSWORD) }
    private fun login() {
        title("登录", "登录后，控制电脑或手机，也可共享这台手机。")
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
        ui.row(body, "共享这台手机", if (CaptureService.running) "正在共享 · 管理共享与控制权限" else "屏幕共享与远程控制权限") { go(Page.SHARING) }
        ui.row(body, "服务器设置", if (hasServer()) app.state.optString("server") else "尚未设置") { go(Page.SERVER) }
        ui.row(body, "软件更新", "当前版本 ${app.updates.currentVersion}") { go(Page.UPDATES) }
        ui.row(body, "后台运行", "管理手机的后台与电池设置") {
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.parse("package:$packageName")))
        }
        if (signedIn()) ui.link(body, "退出登录") { stop(); RemotePreferences.clear(this); perform("logout", success = "") { history.clear(); go(Page.LOGIN, true) } }
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
        ui.text(body, "我的设备", 28f, bold = true); ui.gap(body, 16)
        incoming = ui.link(body, "") { go(Page.SHARING) }.apply { visibility = View.GONE }
        if (!bound()) bindPhone(ui.card(body))
        else devices = DeviceListContent(this, ui, body).also { if (foreground) it.resume() }
    }
    private fun bindPhone(card: LinearLayout) {
        if (!signedIn()) {
            ui.text(card, "登录后即可开启本机共享。", 14f, secondary = true); ui.gap(card, 16)
            ui.button(card, "前往登录") { go(Page.LOGIN) }
            return
        }
        val name = ui.field(card, "手机名称", value = "${Build.MANUFACTURER} ${Build.MODEL}")
        primary(card, "添加这台手机") { perform("bind", JSONObject().put("name", name.text.toString().trim()), "") }
    }
    private fun sharing() {
        boundWhenBuilt = bound(); lastRequests = ""
        title("共享这台手机", "允许其他设备查看或控制本机。")
        requestBox = ui.column(body)
        val card = ui.card(body)
        if (!bound()) {
            bindPhone(card)
        } else {
            status = ui.text(card, "", 16f, bold = true); ui.gap(card, 6); sharingDetail = ui.text(card, "", 13f, secondary = true); ui.gap(card, 16)
            share = ui.button(card, "开启屏幕共享") { if (CaptureService.running) { stop(); draw() } else consent() }
            ui.divider(card)
            ui.toggle(card, "共享期间防止自动熄屏", SharingPower.enabled(this)).apply {
                setOnCheckedChangeListener { _, enabled -> SharingPower.preference(this@MainActivity, enabled) }
            }
            ui.text(card, "手动锁屏会中断共享，解锁后需重新授权。", 12f, secondary = true)
            ui.link(card, "共享与锁屏说明") {
                AlertDialog.Builder(this).setTitle("共享与锁屏").setMessage("防止自动熄屏开启后，屏幕可变暗但不会自动熄屏，切到其他应用也有效，会增加耗电。\n\n手动锁屏仍会结束系统录屏授权。解锁后，点击恢复屏幕共享并确认系统授权。\n\n控制其他设备无需开启这台手机的共享或辅助功能。")
                    .setPositiveButton("知道了", null).show()
            }
            ui.divider(card)
            ui.text(card, "远程控制", 17f, bold = true); ui.gap(card, 4)
            control = ui.toggle(card, "允许同账号设备直接控制")
            controlDetail = ui.text(card, "", 13f, secondary = true)
        }
        ui.link(body, "管理远程控制授权") { go(Page.CONTROL_SETUP) }
        ui.gap(body, 16)
        ui.toggle(body, "打开 App 时恢复上次共享", RemotePreferences.autoRestore(this)).apply {
            setOnCheckedChangeListener { _, enabled -> RemotePreferences.autoRestore(this@MainActivity, enabled) }
        }
        ui.text(body, "录屏授权失效时需重新确认；主动停止或取消后不会自动恢复。", 13f, secondary = true)
    }
    private fun updateSharing() {
        val phase = CaptureService.phase
        val remembered = RemotePreferences.wantsSharing(this, app.state)
        status?.text = when (phase) { "consent" -> "等待系统录屏授权"; "starting" -> "正在开启共享…"; "sharing" -> "● 屏幕共享中"; "failed" -> "开启共享失败"; "interrupted" -> "屏幕共享已中断"; else -> if (remembered) "共享待恢复 · 设置已记住" else "○ 屏幕共享未开启" }
        status?.setTextColor(if (phase == "sharing") ui.green else if (phase == "failed") android.graphics.Color.rgb(164, 43, 43) else ui.ink)
        sharingDetail?.text = when (phase) {
            "sharing" -> "其他设备可申请连接，切到后台仍可共享。"
            "off" -> if (remembered) "开启选择已记住，重新确认录屏授权即可恢复。" else "开启后，其他设备可查看或控制这台手机。"
            else -> CaptureService.detail
        }
        share?.text = when (phase) { "consent" -> "等待你确认授权…"; "starting" -> if (CaptureService.running) "取消开启" else "正在开启共享…"; "sharing" -> "停止共享"; "failed" -> "重新开启共享"; "interrupted" -> "恢复屏幕共享"; else -> if (remembered) "恢复屏幕共享" else "开启屏幕共享" }
        share?.isEnabled = app.ready && phase != "consent" && (CaptureService.running || (phase != "starting" && !app.busy))
        share?.apply {
            val active = CaptureService.running
            setTextColor(if (active) ui.green else android.graphics.Color.WHITE)
            background = ui.shape(if (active) android.graphics.Color.rgb(232, 240, 234) else ui.green, 13)
        }
        controlDetail?.text = if (ControlService.instance == null) { if (RemotePreferences.wantsControl(this, app.state)) "选择已记住，等待系统辅助功能连接。" else "开启时需授予 FarSail 辅助功能权限。" } else "系统授权已就绪；开启后，同账号设备无需重复确认。"
        control?.apply {
            setOnCheckedChangeListener(null); isEnabled = app.ready; isChecked = RemotePreferences.wantsControl(this@MainActivity, app.state)
            setOnCheckedChangeListener { _, checked ->
                if (checked && ControlService.instance == null) {
                    setControl(true)
                    pendingControlEnable = true; go(Page.CONTROL_SETUP)
                    if (!ControlSetup.open(this@MainActivity)) { say("请在已下载的应用或已安装的服务中选择「FarSail 远程控制」。"); draw() }
                } else setControl(checked)
            }
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
    private fun setControl(enabled: Boolean) {
        RemotePreferences.control(this, app.state, enabled); ControlService.cancel()
        app.state.put("control", enabled && ControlService.instance != null && RemotePreferences.wantsControl(this, app.state))
        app.io.execute { try { app.refresh() } catch (_: Exception) { } }
    }
    private fun controlSetup() {
        guideConnected = ControlService.instance != null
        title("远程控制授权", if (guideConnected) "✓ FarSail 辅助功能已连接" else "只需开启「FarSail 远程控制」这一项。")
        val card = ui.card(body)
        ui.text(card, "FarSail 远程控制", 20f, bold = true); ui.gap(card, 12)
        ui.text(card, if (guideConnected) "授权已完成。开启控制后，同账号设备无需在手机再次确认；其他账号的请求仍需确认。" else "接下来会打开并定位 FarSail 的系统授权。若看到服务列表，点击「FarSail 远程控制」；再打开使用开关、确认系统提示，返回 App 即可。", 14f)
        ui.gap(card, 20)
        if (guideConnected) ui.button(card, if (signedIn() && bound()) "开启远程控制" else "完成") {
            pendingControlEnable = false
            if (signedIn() && bound()) setControl(true)
            if (!back()) go(if (signedIn()) Page.HOME else Page.LOGIN, true)
        } else ui.button(card, "去开启 FarSail 远程控制") {
            pendingControlEnable = signedIn() && bound()
            if (pendingControlEnable) setControl(true)
            if (!ControlSetup.open(this)) { say("请在已下载的应用或已安装的服务中选择「FarSail 远程控制」。"); draw() }
        }
        ui.gap(body, 16); ui.text(body, "如果系统没有直达：辅助功能 → 已下载的应用 / 已安装的服务 → FarSail 远程控制。", 13f, secondary = true)
        ui.gap(body, 12); ui.text(body, "如果系统提示受限设置，请先在应用信息中按系统提示允许此应用的受限设置，再返回授权。", 13f, secondary = true)
        ui.link(body, "打开 FarSail 应用信息") { startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.parse("package:$packageName"))) }
        ui.link(body, "暂不开启") { if (pendingControlEnable && signedIn() && bound()) setControl(false); pendingControlEnable = false; if (!back()) go(if (signedIn()) Page.HOME else Page.LOGIN, true) }
    }
    private fun stop() { NativeBridge.stopLocal(); ControlService.cancel(); CaptureService.stop(this) }
    private fun consent(restoring: Boolean = false) {
        consentTicket = RemotePreferences.beginConsent(this, app.state)
        if (consentTicket == 0L) return
        consentOutstanding = true; restoreSharingPending = false
        if (restoring && page != Page.SHARING) go(Page.SHARING)
        CaptureService.status(this, "consent", if (restoring) "正在恢复上次共享，请在系统弹窗中确认共享整个屏幕。" else "请在系统弹窗中确认共享整个屏幕。")
        draw()
        val manager = getSystemService(MediaProjectionManager::class.java)
        val intent = if (Build.VERSION.SDK_INT >= 34) manager.createScreenCaptureIntent(MediaProjectionConfig.createConfigForDefaultDisplay()) else manager.createScreenCaptureIntent()
        try { @Suppress("DEPRECATION") startActivityForResult(intent, 42) }
        catch (e: Exception) { consentOutstanding = false; CaptureService.status(this, "failed", app.friendly(e.message ?: "系统无法打开录屏授权。")); draw() }
    }
    @Deprecated("Framework result callback") override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != 42 || isFinishing || isDestroyed) return
        if (resultCode != RESULT_OK || data == null) {
            consentOutstanding = false
            // Cancellation must survive even if identity restoration is still pending.
            if (RemotePreferences.pendingConsent(this, consentTicket)) {
                RemotePreferences.stopSharing(this)
                CaptureService.status(this, "off", "已取消系统录屏授权，屏幕未共享。")
            }
        } else {
            pendingProjection = ProjectionResult(consentTicket, data)
        }
        draw()
    }
    private fun consumeProjectionResult() {
        val result = pendingProjection ?: return
        if (!app.ready || app.busy) return
        pendingProjection = null; consentOutstanding = false
        if (!RemotePreferences.currentConsent(this, app.state, result.ticket)) {
            if (!CaptureService.running && RemotePreferences.abandonConsent(this, result.ticket))
                CaptureService.status(this, "off", "共享请求已过期，请在当前账号下重新开启。")
            return
        }
        if (RemotePreferences.sharing(this, app.state, result.ticket)) {
            CaptureService.status(this, "starting", "已获得系统授权，正在启动屏幕共享…")
            try { startForegroundService(Intent(this, CaptureService::class.java).putExtra("result", RESULT_OK).putExtra("data", result.data)) }
            catch (e: Exception) { CaptureService.status(this, "failed", app.friendly(e.message ?: "系统无法启动录屏服务。")) }
        }
    }
}
