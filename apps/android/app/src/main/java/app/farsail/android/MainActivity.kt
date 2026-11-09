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
    private enum class Page { LOGIN, REGISTER, VERIFY, SERVER, SETTINGS, SHARING, UPDATES, CONTROL_SETUP, BACKGROUND_SETUP, HOME }
    private val app get() = application as FarSailApp
    private lateinit var ui: MobileUi
    private lateinit var body: LinearLayout
    private lateinit var notice: TextView
    private var foreground = false
    private var restoreSharingPending = true
    private var restoreSharingQueued = false
    private var consentOutstanding = false
    private var returningFromConsent = false
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
    private var forgetControl: Button? = null
    private var connectionDetail: TextView? = null
    private var backgroundDetail: TextView? = null
    private var pendingControlEnable = false
    private var guideConnected = false
    private var share: Button? = null
    private var control: Switch? = null
    private var requestBox: LinearLayout? = null
    private var lastRequests = ""
    private var boundWhenBuilt = false
    private var updateNotice: UpdateNoticeView? = null
    private var updateText: TextView? = null
    private var updateButton: Button? = null
    private var updateProgress: ProgressBar? = null
    private var devices: DeviceListContent? = null
    private var incoming: Button? = null
    private var localIdentity: LocalDeviceIdentity? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ui = MobileUi(this)
        email = savedInstanceState?.getString("email") ?: ""
        pendingControlEnable = savedInstanceState?.getBoolean("pendingControlEnable") ?: false
        consentOutstanding = savedInstanceState?.getBoolean("consentOutstanding") ?: false
        returningFromConsent = consentOutstanding
        consentTicket = savedInstanceState?.getLong("consentTicket") ?: 0
        page = savedInstanceState?.getString("page")?.let { runCatching { Page.valueOf(it) }.getOrNull() } ?: Page.LOGIN
        body = ui.screen()
        draw()
    }
    override fun onResume() {
        super.onResume(); foreground = true
        ConnectionService.retryOnForeground()
        val consentReturn = returningFromConsent; returningFromConsent = false
        if (!consentReturn && !consentOutstanding && CaptureService.phase !in listOf("consent", "starting")) restoreSharingPending = true
        app.observer = { draw() }; app.restoreControl(); draw(); devices?.resume(); app.updates.maybeCheck()
        if (page == Page.SHARING) localIdentity?.refresh()
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
        if (op == "logout") ConnectionService.stop(this)
        app.act(op, args, success, owner.name, { !isFinishing && !isDestroyed && page == owner && navigationEpoch == token }, after)
    }
    private fun say(text: String) { app.messageScope = page.name; app.message = text }
    private fun signedIn() = app.state.optBoolean("signedIn")
    private fun hasServer() = app.state.optString("server").let { it.isNotBlank() && it != "http://127.0.0.1:8787" }
    private fun bound() = app.state.optString("deviceId").let { it.isNotBlank() && it != "null" }
    private fun primary(parent: LinearLayout, title: String, action: () -> Unit) = ui.button(parent, title, action = action).also { busyButtons.add(it); it.isEnabled = app.ready && !app.busy }
    private fun draw() {
        consumeProjectionResult()
        if (pendingControlEnable && ControlSetup.connected(this) && app.ready && signedIn() && bound()) {
            pendingControlEnable = false
            setControl(true)
            if (history.peekLast() == Page.SHARING) history.removeLast()
            page = Page.SHARING; rendered = null
            say("远程控制已开启，同账号设备无需手机重复确认。")
        }
        if (page == Page.CONTROL_SETUP && guideConnected != ControlSetup.connected(this)) rendered = null
        if (signedIn() && page in listOf(Page.LOGIN, Page.REGISTER, Page.VERIFY)) { page = Page.HOME; rendered = null; history.clear() }
        if (!signedIn() && page == Page.HOME) { page = Page.LOGIN; rendered = null; history.clear() }
        if (page in listOf(Page.HOME, Page.SHARING) && boundWhenBuilt != bound()) rendered = null
        if (rendered != page) {
            devices?.close(); devices = null; incoming = null
            localIdentity = null
            rendered = page; body.removeAllViews(); busyButtons.clear(); authEmail = null
            (body.parent as ScrollView).scrollTo(0, 0)
            status = null; sharingDetail = null; controlDetail = null; forgetControl = null; connectionDetail = null; backgroundDetail = null; share = null; control = null; requestBox = null; updateText = null; updateButton = null; updateProgress = null
            header()
            updateNotice = if (page in listOf(Page.HOME, Page.SETTINGS, Page.LOGIN)) UpdateNoticeView(this) {
                when (app.updates.state.phase) {
                    "available" -> app.updates.download()
                    "ready" -> app.updates.install(this)
                    else -> go(Page.UPDATES)
                }
            }.also { body.addView(it) } else null
            when (page) {
                Page.LOGIN -> login()
                Page.REGISTER -> register()
                Page.VERIFY -> verify()
                Page.SERVER -> server()
                Page.SETTINGS -> settings()
                Page.SHARING -> sharing()
                Page.UPDATES -> updates()
                Page.CONTROL_SETUP -> controlSetup()
                Page.BACKGROUND_SETUP -> backgroundSettings()
                Page.HOME -> home()
            }
        }
        busyButtons.forEach { it.isEnabled = app.ready && !app.busy; it.alpha = if (it.isEnabled) 1f else .55f }
        notice.text = if (!app.ready && app.message.isBlank()) "正在恢复已保存的设置…" else if (app.busy && app.busyScope == page.name) "正在处理…" else if (app.messageScope == page.name) app.message else ""
        notice.visibility = if (notice.text.isNullOrEmpty()) View.GONE else View.VISIBLE
        if (page == Page.SHARING) updateSharing()
        localIdentity?.render()
        incoming?.apply {
            val count = app.pending.length()
            visibility = if (count > 0) View.VISIBLE else View.GONE
            text = "收到 $count 个连接请求 · 处理"
        }
        updateNotice?.render(app.updates.state)
        if (page == Page.UPDATES) updateUpdates()
        if (app.ready && !app.busy) ConnectionService.synchronize(this, app.state, foreground)
        connectionDetail?.text = if (ConnectionService.enabled(this)) ConnectionService.detail else "仅共享或连接期间保持在线。"
        backgroundDetail?.text = BackgroundSetup.detail(this)
        maybeRestoreSharing()
    }
    private fun maybeRestoreSharing() {
        if (CaptureService.running) { restoreSharingPending = false; return }
        if (restoreSharingQueued || !canRestoreSharing()) return
        restoreSharingQueued = true
        app.main.post {
            restoreSharingQueued = false
            // A busy request or navigation must not consume the foreground attempt.
            // Explicit Stop/cancel still clears the saved sharing choice.
            if (!isFinishing && !isDestroyed && canRestoreSharing()) {
                restoreSharingPending = false
                consent(true)
            }
        }
    }
    private fun canRestoreSharing() = foreground && restoreSharingPending && !consentOutstanding &&
        (Build.VERSION.SDK_INT < 30 || !RemotePreferences.accessibilityCapture(this, app.state) || AccessibilityFrames.available(this)) &&
        page in listOf(Page.HOME, Page.SETTINGS, Page.SHARING, Page.UPDATES) && app.ready && !app.busy &&
        signedIn() && bound() && !CaptureService.running && CaptureService.phase in listOf("off", "interrupted", "failed") &&
        RemotePreferences.autoRestore(this) && RemotePreferences.wantsSharing(this, app.state)
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
        ui.toggle(body, "保持本机在线", ConnectionService.enabled(this)).apply {
            setOnCheckedChangeListener { _, enabled -> ConnectionService.preference(this@MainActivity, enabled) }
        }
        connectionDetail = ui.text(body, "", 13f, secondary = true)
        ui.text(body, "锁屏或停止共享后仍保持设备连接，可能增加耗电；查看屏幕仍需开启共享并取得系统授权。", 13f, secondary = true)
        backgroundDetail = ui.text(body, "", 13f, secondary = true)
        ui.row(body, "锁屏与后台设置", "电池优化、后台活动、自启动与辅助功能") { go(Page.BACKGROUND_SETUP) }
        if (signedIn()) ui.link(body, "退出登录") { stop(); RemotePreferences.clear(this); perform("logout", success = "") { history.clear(); go(Page.LOGIN, true) } }
    }
    private fun backgroundSettings() {
        title("锁屏与后台设置", "${Build.MANUFACTURER} ${Build.MODEL} · Android ${Build.VERSION.RELEASE}")
        val online = ui.card(body)
        ui.text(online, "保持设备在线", 18f, bold = true)
        ui.text(online, "先在设置中开启「保持本机在线」。在线仅表示设备可连接，查看画面还需要已开启屏幕共享。", 13f, secondary = true)
        backgroundDetail = ui.text(online, "", 13f, secondary = true)
        ui.gap(online, 12)
        ui.button(online, "设置电池优化豁免") { BackgroundSetup.open(this) }
        ui.button(online, "打开应用后台设置", false) { BackgroundSetup.openAppDetails(this) }
        if (BackgroundSetup.colorOs()) {
            ui.text(online, "OPPO / ColorOS：在应用详情的「耗电管理」或系统设置的「电池 → 应用耗电管理」找到 FarSail，允许后台运行；若有自启动选项也请允许。旧版系统可能在「手机管家 → 权限隐私 → 自启动管理」。菜单名称随版本变化，请以实际页面为准。", 13f, secondary = true)
            ui.button(online, "打开手机管家", false) { BackgroundSetup.openPhoneManager(this) }
            ui.text(online, "系统电池豁免不能代表厂商后台和自启动已放行；这些开关仍需你手动确认。", 13f, secondary = true)
        } else ui.text(online, "在应用详情中查找电池、后台活动或自启动设置，按手机实际菜单允许 FarSail 在后台运行。", 13f, secondary = true)
        ui.gap(body, 16)
        val capture = ui.card(body)
        ui.text(capture, "画面与远程控制", 18f, bold = true)
        ui.text(capture, BackgroundSetup.captureDetail(), 13f, secondary = true)
        ui.button(capture, "管理辅助功能授权", false) { go(Page.CONTROL_SETUP) }
        ui.button(capture, "管理屏幕共享", false) { go(Page.SHARING) }
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
        body.addView(Switch(this).apply { text = "仅 Wi-Fi 自动下载"; isChecked = app.updates.wifiOnly; setPadding(0, ui.dp(12), 0, ui.dp(12)); setOnCheckedChangeListener { _, v -> app.updates.preferences(app.updates.autoCheck, app.updates.autoDownload, v) } })
        ui.gap(body, 10); ui.text(body, "发现新版本会在首页提示。默认在非计费 Wi-Fi 下自动下载，安装由手机系统确认。安装前请结束屏幕共享。", 13f, secondary = true)
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
        if (bound()) localIdentity = LocalDeviceIdentity(this, ui, body)
        incoming = ui.link(body, "") { go(Page.SHARING) }.apply { visibility = View.GONE }
        if (bound()) ui.link(body, "锁屏与后台设置") { go(Page.BACKGROUND_SETUP) }
        if (bound() && Build.VERSION.SDK_INT >= 30 && !RemotePreferences.accessibilityCapture(this, app.state))
            ui.link(body, "启用免重复确认共享") { chooseAccessibilityCapture() }
        if (!bound()) bindPhone(ui.card(body))
        else devices = DeviceListContent(this, ui, body, localIdentity).also { if (foreground) it.resume() }
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
        if (bound()) {
            localIdentity = LocalDeviceIdentity(this, ui, body).also { if (foreground) it.refresh() }
            ui.link(body, "刷新本机名称") { localIdentity?.refresh() }
        }
        requestBox = ui.column(body)
        val card = ui.card(body)
        if (!bound()) {
            bindPhone(card)
        } else {
            status = ui.text(card, "", 16f, bold = true); ui.gap(card, 6); sharingDetail = ui.text(card, "", 13f, secondary = true); ui.gap(card, 16)
            share = ui.button(card, "开启屏幕共享") { if (CaptureService.running) { stop(); draw() } else consent() }
            if (Build.VERSION.SDK_INT >= 30) ui.link(card, if (RemotePreferences.accessibilityCapture(this, app.state)) "共享方式：辅助功能截图 · 切换为系统录屏" else "切换为免重复确认共享") {
                if (CaptureService.running) { say("请先停止共享，再切换采集方式。"); draw() }
                else if (RemotePreferences.accessibilityCapture(this, app.state)) {
                    RemotePreferences.accessibilityCapture(this, app.state, false); rendered = null; draw()
                } else chooseAccessibilityCapture()
            }
            ui.divider(card)
            ui.toggle(card, "共享期间防止自动熄屏", SharingPower.enabled(this)).apply {
                setOnCheckedChangeListener { _, enabled -> SharingPower.preference(this@MainActivity, enabled) }
            }
            ui.text(card, if (RemotePreferences.accessibilityCapture(this, app.state)) "截图共享不依赖录屏会话；系统保护页面和厂商后台策略仍可能限制共享。" else "已允许的控制连接会尝试唤醒屏幕；锁屏导致录屏中断时仍需在手机上重新授权。", 12f, secondary = true)
            ui.link(card, "共享与锁屏说明") {
                AlertDialog.Builder(this).setTitle("共享与锁屏").setMessage(if (RemotePreferences.accessibilityCapture(this, app.state)) "辅助功能截图不依赖录屏会话，无需重复确认录屏。系统保护页面可能黑屏，锁屏和后台兼容性取决于手机系统；此功能不会绕过密码。\n\n防止自动熄屏会增加耗电，可按需要关闭；手动锁屏不会主动停止截图服务。\n\n控制其他设备无需开启这台手机的共享或辅助功能。" else "防止自动熄屏开启后，屏幕可变暗但不会自动熄屏，切到其他应用也有效，会增加耗电。\n\n已允许的远程控制连接建立时会尝试亮屏，无密码时会请求收起锁屏。有密码时不会解除密码锁。\n\n锁屏后，系统可能停止录屏或限制画面。发生中断时，解锁后点击恢复屏幕共享并确认系统授权。\n\n控制其他设备无需开启这台手机的共享或辅助功能。")
                    .setPositiveButton("知道了", null).show()
            }
            ui.divider(card)
            ui.text(card, "远程控制", 17f, bold = true); ui.gap(card, 4)
            control = ui.toggle(card, "允许同账号设备直接控制")
            controlDetail = ui.text(card, "", 13f, secondary = true)
            forgetControl = ui.link(card, "取消已记住的控制设置") { pendingControlEnable = false; setControl(false); draw() }
        }
        ui.link(body, "管理远程控制授权") { go(Page.CONTROL_SETUP) }
        ui.link(body, "锁屏与后台设置") { go(Page.BACKGROUND_SETUP) }
        ui.gap(body, 16)
        ui.toggle(body, "打开 App 时恢复上次共享", RemotePreferences.autoRestore(this)).apply {
            setOnCheckedChangeListener { _, enabled -> RemotePreferences.autoRestore(this@MainActivity, enabled) }
        }
        ui.text(body, if (RemotePreferences.accessibilityCapture(this, app.state)) "已授权的辅助功能截图可在打开 App 时恢复，无需录屏弹窗。主动停止后保持关闭。" else "开启一次后，打开 App 自动进入系统共享确认，无需再点开启按钮。主动停止或取消授权后保持关闭。", 13f, secondary = true)
    }
    private fun updateSharing() {
        val phase = CaptureService.phase
        val remembered = RemotePreferences.wantsSharing(this, app.state)
        status?.text = when (phase) { "consent" -> "等待系统录屏授权"; "starting" -> "正在开启共享…"; "sharing" -> "● 屏幕共享中"; "failed" -> "开启共享失败"; "interrupted" -> "屏幕共享已中断"; else -> if (remembered) "共享待恢复 · 设置已记住" else "○ 屏幕共享未开启" }
        status?.setTextColor(if (phase == "sharing") ui.green else if (phase == "failed") android.graphics.Color.rgb(164, 43, 43) else ui.ink)
        sharingDetail?.text = when (phase) {
            "sharing" -> if (CaptureService.accessibilityMode) "辅助功能截图共享中，无需重复确认录屏；受保护页面可能无法显示。" else "其他设备可申请连接，切到后台仍可共享。"
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
        val access = ControlSetup.status(this, app.state)
        controlDetail?.text = access.detail
        forgetControl?.apply { visibility = if (access.remembered && !access.active) View.VISIBLE else View.GONE; isEnabled = app.ready && !app.busy }
        control?.apply {
            setOnCheckedChangeListener(null); isEnabled = app.ready && !app.busy; isChecked = access.active
            setOnCheckedChangeListener { _, checked ->
                if (checked && !ControlSetup.connected(this@MainActivity)) {
                    setControl(true)
                    pendingControlEnable = true; go(Page.CONTROL_SETUP)
                    if (!ControlSetup.open(this@MainActivity)) { say("请在已下载的应用或已安装的服务中选择「FarSail 远程控制」。"); draw() }
                } else setControl(checked)
            }
        }
        val snapshot = app.pending.toString() + (app.state.optJSONObject("session")?.toString() ?: "null") + "|${access.active}|${access.directConnection}"
        if (snapshot == lastRequests) return
        lastRequests = snapshot; val box = requestBox ?: return; busyButtons.removeAll { it.parent === box }; box.removeAllViews()
        app.state.optJSONObject("session")?.let { ui.text(box, "已连接 · ${if (it.optString("permission") == "control") "可控制" else "仅查看"}", 18f, bold = true); ui.text(box, "核对码：${it.optString("code", "—")}", 14f, secondary = true); ui.gap(box, 16) }
        for (i in 0 until app.pending.length()) {
            val r = app.pending.getJSONObject(i); ui.text(box, r.optString("source_device_name", "电脑"), 18f, bold = true); ui.text(box, "${r.optString("requester_email")} 请求${if (r.optString("permission") == "control") "控制手机" else "查看屏幕"}", 14f, secondary = true); ui.gap(box, 16)
            if (r.optString("permission") == "control" && !access.active) {
                ui.text(box, "控制权限尚未生效，请先恢复授权；同账号免确认也需要该权限。", 13f, secondary = true)
                primary(box, "恢复控制权限") { go(Page.CONTROL_SETUP) }
            } else primary(box, "允许连接") { perform("decide", JSONObject().put("id", r.getString("id")).put("approve", true), "已批准连接") }
            ui.button(box, "拒绝", false) { perform("decide", JSONObject().put("id", r.getString("id")).put("approve", false), "已拒绝连接") }
        }
    }
    private fun setControl(enabled: Boolean) {
        RemotePreferences.control(this, app.state, enabled); ControlService.cancel()
        app.state.put("control", enabled && ControlSetup.connected(this) && RemotePreferences.wantsControl(this, app.state))
        app.io.execute { try { app.refresh() } catch (_: Exception) { } }
    }
    private fun controlSetup() {
        guideConnected = ControlSetup.connected(this)
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
    private fun chooseAccessibilityCapture() {
        if (CaptureService.running) { say("请先停止共享，再切换采集方式。"); draw(); return }
        AlertDialog.Builder(this).setTitle("免重复确认共享")
            .setMessage("使用系统辅助功能截图，需在系统设置中一次性允许 FarSail 远程控制服务。开启共享后会持续取得屏幕画面并显示通知，可随时停止。\n\n此方式约每秒两帧。已允许的远程控制连接会尝试亮屏，无密码时请求收起锁屏。连接后再次熄屏时，首次远程点击仅用于唤醒，请再次点击操作；不会解除密码锁，受保护页面可能黑屏。锁屏及后台兼容性取决于手机系统，无需 ADB 激活。")
            .setPositiveButton("启用并共享") { _, _ ->
                RemotePreferences.accessibilityCapture(this, app.state, true)
                go(Page.SHARING); rendered = null; restoreSharingPending = false; consent()
            }.setNegativeButton("取消", null).show()
    }
    private fun consent(restoring: Boolean = false) {
        consentTicket = RemotePreferences.beginConsent(this, app.state)
        if (consentTicket == 0L) return
        if (Build.VERSION.SDK_INT >= 30 && RemotePreferences.accessibilityCapture(this, app.state)) {
            restoreSharingPending = false
            if (!AccessibilityFrames.available(this)) {
                CaptureService.status(this, "interrupted", "请在系统设置中允许 FarSail 远程控制服务；返回后可用辅助功能截图共享。")
                draw(); if (!restoring) ControlSetup.open(this); return
            }
            if (RemotePreferences.sharing(this, app.state, consentTicket)) {
                CaptureService.status(this, "starting", "正在开启辅助功能截图共享…")
                try { startForegroundService(Intent(this, CaptureService::class.java).setAction(CaptureService.ACCESSIBILITY)) }
                catch (e: Exception) { CaptureService.status(this, "failed", app.friendly(e.message ?: "系统无法启动截图共享。")) }
            }
            draw(); return
        }
        consentOutstanding = true; returningFromConsent = true; restoreSharingPending = false
        if (restoring && page != Page.SHARING) go(Page.SHARING)
        CaptureService.status(this, "consent", if (restoring) "正在恢复上次共享，请在系统弹窗中确认共享整个屏幕。" else "请在系统弹窗中确认共享整个屏幕。")
        draw()
        val manager = getSystemService(MediaProjectionManager::class.java)
        val intent = if (Build.VERSION.SDK_INT >= 34) manager.createScreenCaptureIntent(MediaProjectionConfig.createConfigForDefaultDisplay()) else manager.createScreenCaptureIntent()
        try { @Suppress("DEPRECATION") startActivityForResult(intent, 42) }
        catch (e: Exception) { consentOutstanding = false; returningFromConsent = false; CaptureService.status(this, "failed", app.friendly(e.message ?: "系统无法打开录屏授权。")); draw() }
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
