package app.farsail.android

import android.app.Application
import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import org.json.JSONArray
import java.util.concurrent.Executors

class FarSailApp : Application() {
    val io = Executors.newSingleThreadExecutor()
    val main = Handler(Looper.getMainLooper())
    var state = JSONObject()
    var pending = JSONArray()
    var message = ""
    var messageScope = "LOGIN"
    var busyScope = ""
    var busy = false
    var observer: (() -> Unit)? = null
    val updates by lazy { Updates(this) }
    @Volatile var ready = false
    @Volatile private var nativeReady = false
    override fun onCreate() {
        super.onCreate()
        main.postDelayed({ updates.maybeCheck() }, 5000)
        io.execute {
            try {
                unwrap(NativeBridge.initialize(CredentialStore(this), this))
                nativeReady = true
                val saved = invoke("state") as JSONObject
                if (saved.optBoolean("signedIn")) invoke("resume")
            } catch (e: Exception) { message = "初始化失败：${e.message}" }
            // Publish restored identity and readiness together. Permission results
            // can arrive before credential restoration has filled the UI snapshot.
            val restored = if (nativeReady) try {
                RemotePreferences.restoreControl(this, invoke("state") as JSONObject, RemotePreferences.ticket())
                invoke("state") as JSONObject
            } catch (e: Exception) { message = "设置恢复失败：${e.message}"; null } else null
            main.post {
                if (restored != null) state = restored
                ready = nativeReady && restored != null
                observer?.invoke()
            }
        }
    }
    fun restoreControl() {
        val ticket = RemotePreferences.ticket()
        io.execute {
            if (!nativeReady) return@execute
            try { RemotePreferences.restoreControl(this, invoke("state") as JSONObject, ticket); refresh() }
            catch (_: Exception) { }
        }
    }
    fun unwrap(raw: String): Any? {
        val r = JSONObject(raw)
        check(r.optBoolean("ok")) { r.optString("error", "操作失败") }
        return r.opt("value")
    }
    fun invoke(op: String, args: JSONObject = JSONObject()): Any? = unwrap(NativeBridge.call(op, args.toString()))
    fun refresh() {
        val next = invoke("state") as JSONObject
        val requests = if (next.optBoolean("sharing")) invoke("pending") as? JSONArray ?: JSONArray() else JSONArray()
        main.post { state = next; pending = requests; observer?.invoke() }
    }
    fun act(op: String, args: JSONObject = JSONObject(), success: String = "", scope: String = "HOME", current: () -> Boolean = { true }, after: (() -> Unit)? = null) {
        if (busy || !ready) return
        busy = true; busyScope = scope; observer?.invoke()
        val restoreTicket = RemotePreferences.ticket()
        io.execute {
            var okay = false
            val result = try {
                invoke(op, args)
                if (op in listOf("login", "resume", "bind", "server")) RemotePreferences.restoreControl(this, invoke("state") as JSONObject, restoreTicket)
                okay = true; success
            } catch (e: Exception) { friendly(e.message ?: "操作失败") }
            try { refresh() } catch (_: Exception) { }
            main.post {
                busy = false
                val stillCurrent = current()
                if (stillCurrent) { message = result; messageScope = scope }
                observer?.invoke()
                if (okay && stillCurrent) after?.invoke()
            }
        }
    }
    fun friendly(s: String) = when {
        s.contains("invalid server URL", true) || s.contains("use HTTPS", true) || s.contains("server host required", true) -> "请填写有效的 HTTPS 服务地址。"
        s.contains("409") -> "操作状态已变化，请刷新后重试。"
        s.contains("401") -> "登录已过期或账号密码不正确，请重新登录。"
        s.contains("403") -> "当前操作未获授权，请检查账号或邮箱验证。"
        s.contains("network", true) || s.contains("connect", true) -> "连接失败，请检查网络和服务地址。"
        else -> s.take(200)
    }
}
