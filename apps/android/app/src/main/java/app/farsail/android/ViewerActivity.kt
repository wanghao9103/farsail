package app.farsail.android

import android.app.Activity
import android.app.AlertDialog
import android.graphics.BitmapFactory
import android.graphics.Color
import android.os.Bundle
import android.text.InputType
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.widget.*
import org.json.JSONObject
import java.nio.ByteBuffer
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.Executors
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** A foreground controller owns one cancellable outgoing connection. */
class ViewerActivity : Activity() {
    private val app get() = application as FarSailApp
    private lateinit var ui: MobileUi
    private lateinit var canvas: RemoteCanvas
    private lateinit var status: TextView
    private lateinit var end: Button
    private lateinit var scroll: Button
    private lateinit var rightClick: Button
    private lateinit var phoneKeys: LinearLayout
    private val controls = mutableListOf<Button>()
    private val connectWorker = Executors.newSingleThreadExecutor()
    private val decoder = Executors.newSingleThreadScheduledExecutor()
    private val commands = ThreadPoolExecutor(1, 1, 0, TimeUnit.MILLISECONDS, ArrayBlockingQueue<Runnable>(32))
    private val posted = AtomicBoolean(false)
    @Volatile private var ticket = 0L
    @Volatile private var active = false
    private var foreground = false
    private var started = false
    private var generation = 0L
    private var remote = JSONObject()
    private var permission = "view"
    private var target = ""
    @Suppress("DEPRECATION")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState); ui = MobileUi(this)
        target = intent.getStringExtra("target") ?: ""
        permission = intent.getStringExtra("permission")?.takeIf { it in listOf("view", "control") } ?: "view"
        val body = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setBackgroundColor(Color.rgb(24, 35, 39)) }
        setContentView(body)
        window.decorView.systemUiVisibility = 0
        window.statusBarColor = Color.rgb(24, 35, 39)
        window.navigationBarColor = Color.rgb(24, 35, 39)
        if (android.os.Build.VERSION.SDK_INT >= 30) window.insetsController?.setSystemBarsAppearance(0, WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS)
        body.setOnApplyWindowInsetsListener { _, insets ->
            if (android.os.Build.VERSION.SDK_INT >= 30) { val bars = insets.getInsets(WindowInsets.Type.systemBars()); body.setPadding(bars.left, bars.top, bars.right, bars.bottom) }; insets
        }
        val header = LinearLayout(this).apply { gravity = android.view.Gravity.CENTER_VERTICAL; setPadding(ui.dp(8), 0, ui.dp(8), 0) }
        body.addView(header, LinearLayout.LayoutParams(-1, ui.dp(54)))
        small(header, "退出") { finish() }
        header.addView(TextView(this).apply { text = intent.getStringExtra("name")?.take(80) ?: "远程设备"; textSize = 17f; setTextColor(Color.WHITE); maxLines = 1 }, LinearLayout.LayoutParams(0, -2, 1f))
        end = small(header, "取消连接") { if (active) disconnect("连接已结束") else connect() }
        status = TextView(this).apply { textSize = 12f; setTextColor(Color.rgb(186, 207, 203)); setPadding(ui.dp(12), ui.dp(5), ui.dp(12), ui.dp(5)); maxLines = 3 }
        body.addView(status)
        canvas = RemoteCanvas(this) { input -> send(input) }
        body.addView(canvas, LinearLayout.LayoutParams(-1, 0, 1f))
        val scroller = HorizontalScrollView(this).apply { isHorizontalScrollBarEnabled = false }
        val actions = LinearLayout(this).apply { setPadding(ui.dp(6), ui.dp(5), ui.dp(6), ui.dp(5)) }
        scroller.addView(actions); body.addView(scroller)
        scroll = small(actions, "滚动模式") {
            val enabled = canvas.scrollMode()
            scroll.text = if (enabled) "滚动中" else "滚动模式"
            scroll.isSelected = enabled
            scroll.backgroundTintList = android.content.res.ColorStateList.valueOf(if (enabled) ui.green else Color.rgb(44, 62, 66))
        }.also(controls::add)
        rightClick = small(actions, "右键") { canvas.rightClick() }.also(controls::add)
        small(actions, "键盘") { keyboard() }.also(controls::add)
        small(actions, "旋转画面") { send(null); canvas.rotate() }
        val more = small(actions, "更多") { }
        more.setOnClickListener {
            val connected = active && remote.optString("phase") == "connected"
            val menu = PopupMenu(this, more)
            menu.menu.add("选择屏幕").apply { isEnabled = connected && (remote.optJSONArray("displays")?.length() ?: 0) > 1; setOnMenuItemClickListener { selectScreen(); true } }
            menu.menu.add("重试控制").apply { isEnabled = connected && permission == "control"; setOnMenuItemClickListener { send(JSONObject().put("kind", "resume_control")); true } }
            menu.show()
        }
        phoneKeys = LinearLayout(this).apply { setPadding(ui.dp(6), 0, ui.dp(6), ui.dp(6)); visibility = View.GONE }
        body.addView(phoneKeys)
        for ((label, vk) in listOf("返回" to 166, "主屏幕" to 172, "最近任务" to 93)) {
            small(phoneKeys, label) { key(vk) }.also { it.layoutParams = LinearLayout.LayoutParams(0, ui.dp(48), 1f); controls.add(it) }
        }
        decoder.scheduleWithFixedDelay({ poll() }, 0, 150, TimeUnit.MILLISECONDS)
        update(JSONObject().put("phase", "idle"))
    }
    private fun small(parent: LinearLayout, text: String, action: () -> Unit): Button = Button(this).also {
        it.text = text; it.isAllCaps = false; it.textSize = 12f; it.minWidth = ui.dp(64); it.setTextColor(Color.WHITE)
        it.setPadding(ui.dp(6), 0, ui.dp(6), 0)
        it.backgroundTintList = android.content.res.ColorStateList.valueOf(Color.rgb(44, 62, 66)); it.setOnClickListener { action() }
        parent.addView(it, LinearLayout.LayoutParams(-2, ui.dp(48)))
    }
    override fun onStart() { super.onStart(); foreground = true; if (!started) { started = true; connect() } }
    override fun onPause() { canvas.release(); send(null); super.onPause() }
    override fun onStop() { foreground = false; disconnect("已离开远程画面，连接已结束"); super.onStop() }
    override fun onDestroy() { disconnect("连接已结束"); decoder.shutdownNow(); connectWorker.shutdownNow(); commands.shutdownNow(); canvas.clear(); super.onDestroy() }
    private fun connect() {
        if (!foreground || !app.ready || active) return
        disconnect(""); canvas.clear(); ticket = NativeBridge.viewerBegin()
        if (ticket <= 0) { status.text = "正在初始化，请稍后重试。"; return }
        active = true; val owner = ticket; end.text = "取消连接"; generation = 0
        update(JSONObject().put("phase", "requesting"))
        connectWorker.execute {
            try {
                if (!current(owner)) return@execute
                val state = app.invoke("viewer_connect", JSONObject().put("ticket", owner).put("target", target).put("permission", permission)) as JSONObject
                app.main.post { if (current(owner)) update(state) }
            } catch (e: Exception) { app.main.post { if (current(owner)) disconnect(app.friendly(e.message ?: "连接失败")) } }
        }
    }
    private fun current(owner: Long) = active && ticket == owner && !isFinishing && !isDestroyed
    private fun disconnect(message: String) {
        active = false; val old = ticket; ticket = 0
        if (old > 0) NativeBridge.viewerClose(old)
        commands.queue.clear()
        if (::canvas.isInitialized) canvas.control = false
        if (::end.isInitialized) { end.text = "重新连接"; controls.forEach { it.isEnabled = false }; status.text = message }
    }
    private fun poll() {
        val owner = ticket
        if (!current(owner) || !posted.compareAndSet(false, true)) return
        try {
            val state = app.invoke("viewer_state", JSONObject().put("ticket", owner)) as JSONObject
            val bytes = NativeBridge.viewerFrame(owner)
            val image = if (bytes != null && bytes.size > 49) {
                val header = ByteBuffer.wrap(bytes)
                val options = BitmapFactory.Options()
                var edge = maxOf(header.getInt(33), header.getInt(37))
                while (edge > 2048) { options.inSampleSize = maxOf(1, options.inSampleSize) * 2; edge /= 2 }
                val bitmap = BitmapFactory.decodeByteArray(bytes, 49, bytes.size - 49, options) ?: error("远程画面解码失败")
                RemoteCanvas.Frame(bitmap, header.getInt(5), header.getLong(9), header.getLong(17))
            } else null
            app.main.post {
                try {
                    if (current(owner)) { if (image != null) canvas.show(image); update(state) }
                    else image?.bitmap?.recycle()
                } finally { posted.set(false) }
            }
        } catch (e: Exception) {
            posted.set(false)
            app.main.post { if (current(owner)) disconnect(app.friendly(e.message ?: "连接已结束")) }
        }
    }
    private fun update(state: JSONObject) {
        remote = state; generation = state.optJSONObject("input")?.optLong("generation") ?: 0
        val phase = state.optString("phase")
        val android = state.optString("platform") == "android"
        val blocked = state.optJSONObject("input")?.optBoolean("blocked") == true
        val enabled = active && phase == "connected" && state.optBoolean("control")
        canvas.control = enabled; controls.forEach { it.isEnabled = enabled }
        phoneKeys.visibility = if (android) View.VISIBLE else View.GONE
        rightClick.visibility = if (android) View.GONE else View.VISIBLE
        status.text = when (phase) {
            "requesting" -> "正在请求连接…"
            "waiting" -> "等待对方批准${if (permission == "control") "控制" else "查看"}…"
            "connecting" -> "已获批准，正在建立连接…"
            "connected" -> {
                val path = if (state.optString("path") == "direct") "直连" else "中继"
                val mode = if (permission == "view") "仅查看" else if (blocked) "控制已暂停" else if (enabled) "可控制" else "等待画面"
                val code = state.optString("verification_code").takeUnless { it.isBlank() || it == "null" }
                "已连接 · $path · $mode" + (code?.let { "\n核对码：$it" } ?: "")
            }
            "closed" -> state.optString("error", "连接已结束").takeUnless { it == "null" } ?: "连接已结束"
            else -> "请选择设备连接"
        }
        if (phase == "connected") end.text = "结束连接"
        if (phase == "closed" && active) disconnect(status.text.toString())
    }
    private fun send(input: JSONObject?) {
        val owner = ticket; val observed = generation
        if (!current(owner) || permission != "control") return
        if (input?.optString("kind") == "move" && commands.queue.size >= 24) return
        val args = JSONObject().put("ticket", owner).put("generation", observed).put("input", input ?: JSONObject.NULL)
        try {
            commands.execute {
                if (!current(owner)) return@execute
                try { app.invoke("viewer_input", args) }
                catch (e: Exception) { app.main.post { if (current(owner)) disconnect(app.friendly(e.message ?: "控制已停止")) } }
            }
        } catch (_: java.util.concurrent.RejectedExecutionException) {
            NativeBridge.viewerClose(owner); disconnect("操作队列已满，连接已安全结束，请重新连接。")
        }
    }
    private fun key(vk: Int) { send(JSONObject().put("kind", "key").put("vk", vk).put("down", true)); send(JSONObject().put("kind", "key").put("vk", vk).put("down", false)) }
    private fun keyboard() {
        if (!canvas.control) return
        canvas.release()
        val labels = if (remote.optString("platform") == "android")
            arrayOf("输入文字", "回车", "退格", "全选", "返回")
        else arrayOf("输入文字", "回车", "退格", "Tab", "Esc", "Ctrl+C", "Ctrl+V", "Windows 键")
        AlertDialog.Builder(this).setTitle("键盘操作").setItems(labels) { _, which ->
            when (labels[which]) {
                "输入文字" -> {
                    val input = EditText(this).apply { inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE; isSaveEnabled = false; setAutofillHints(null); hint = "输入要发送的文字" }
                    AlertDialog.Builder(this).setTitle("发送文字").setView(input).setNegativeButton("取消", null).setPositiveButton("发送") { _, _ ->
                        val text = input.text.toString()
                        if (text.toByteArray(Charsets.UTF_8).size > 768) Toast.makeText(this, "文字过长，请分段发送。", Toast.LENGTH_SHORT).show()
                        else if (text.isNotEmpty()) send(JSONObject().put("kind", "text").put("text", text))
                        input.text.clear()
                    }.show()
                }
                "回车" -> key(13); "退格" -> key(8); "Tab" -> key(9); "Esc", "返回" -> key(27)
                "Ctrl+C", "Ctrl+V", "全选" -> {
                    send(JSONObject().put("kind", "key").put("vk", 17).put("down", true))
                    key(when (labels[which]) { "Ctrl+C" -> 67; "Ctrl+V" -> 86; else -> 65 })
                    send(JSONObject().put("kind", "key").put("vk", 17).put("down", false))
                }
                "Windows 键" -> key(91)
            }
        }.show()
    }
    private fun selectScreen() {
        val list = remote.optJSONArray("displays") ?: return
        if (list.length() < 2) return
        val names = Array(list.length()) { list.getJSONObject(it).optString("name", "屏幕 ${it + 1}") }
        AlertDialog.Builder(this).setTitle("选择远程屏幕").setItems(names) { _, index ->
            val owner = ticket; canvas.clear(); send(null)
            try { commands.execute {
                if (!current(owner)) return@execute
                try { app.invoke("viewer_select", JSONObject().put("ticket", owner).put("display", list.getJSONObject(index).getInt("id"))) }
                catch (e: Exception) { app.main.post { if (current(owner)) disconnect(app.friendly(e.message ?: "切换屏幕失败")) } }
            } } catch (_: java.util.concurrent.RejectedExecutionException) { disconnect("操作队列已满，请重新连接。") }
        }.show()
    }
}
