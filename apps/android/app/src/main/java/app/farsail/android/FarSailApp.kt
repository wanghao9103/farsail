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
    var busy = false
    var observer: (() -> Unit)? = null
    var ready = false
    override fun onCreate() {
        super.onCreate()
        io.execute {
            try {
                unwrap(NativeBridge.initialize(CredentialStore(this), this))
                ready = true
                val saved = invoke("state") as JSONObject
                if (saved.optBoolean("signedIn")) invoke("resume")
                refresh()
            } catch (e: Exception) { message = "初始化失败：${e.message}" }
            try { if (ready) refresh() } catch (_: Exception) { }
            main.post { observer?.invoke() }
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
    fun act(op: String, args: JSONObject = JSONObject(), success: String = "", after: (() -> Unit)? = null) {
        if (busy || !ready) return
        busy = true; observer?.invoke()
        io.execute {
            var okay = false
            try { invoke(op, args); message = success; okay = true } catch (e: Exception) { message = friendly(e.message ?: "操作失败") }
            try { refresh() } catch (_: Exception) { }
            main.post { busy = false; observer?.invoke(); if (okay) after?.invoke() }
        }
    }
    private fun friendly(s: String) = when {
        s.contains("401") -> "登录已过期或账号密码不正确，请重新登录。"
        s.contains("403") -> "当前操作未获授权，请检查账号或邮箱验证。"
        s.contains("network", true) || s.contains("connect", true) -> "连接失败，请检查网络和服务地址。"
        else -> s.take(200)
    }
}
