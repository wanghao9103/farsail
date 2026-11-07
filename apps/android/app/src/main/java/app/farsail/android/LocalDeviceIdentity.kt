package app.farsail.android

import android.app.Activity
import android.widget.LinearLayout
import org.json.JSONArray
import org.json.JSONObject

/** Resolve the registered name by this device's ID, never by its hardware model. */
class LocalDeviceIdentity(private val activity: Activity, ui: MobileUi, body: LinearLayout) {
    private val app get() = activity.application as FarSailApp
    private val name = ui.text(body, "本机名称：正在读取…", 18f, bold = true)
    private val number = ui.text(body, "", 13f, secondary = true)
    private var identity = ""
    init { ui.gap(body, 12); render() }
    private fun key(state: JSONObject): String {
        val id = state.optString("deviceId").takeUnless { it.isBlank() || it == "null" }
        return if (state.optBoolean("signedIn") && id != null) "${state.optString("server")}\n$id" else ""
    }
    fun render() {
        val current = key(app.state)
        if (identity != current) { identity = current; name.text = "本机名称：正在读取…" }
        number.text = if (current.isEmpty()) "本机尚未添加" else "本机编号：${app.state.optString("deviceId").take(8)}"
    }
    fun update(snapshot: JSONObject, devices: JSONArray) {
        render()
        if (key(snapshot).isEmpty() || key(snapshot) != identity) return
        val own = snapshot.optString("deviceId")
        val registered = (0 until devices.length()).mapNotNull { devices.optJSONObject(it) }
            .firstOrNull { it.optString("id") == own }?.optString("name")
            ?.takeUnless { it.isBlank() || it == "null" }
        name.text = if (registered == null) "本机名称：暂未获取，请刷新重试" else "本机名称：$registered"
    }
    fun refresh() {
        render()
        if (!app.ready || identity.isEmpty()) return
        val requested = identity
        app.io.execute {
            val result = runCatching {
                val snapshot = app.invoke("state") as JSONObject
                check(key(snapshot) == requested)
                snapshot to (app.invoke("devices") as JSONArray)
            }
            app.main.post {
                if (activity.isFinishing || activity.isDestroyed || !name.isAttachedToWindow || key(app.state) != requested) return@post
                result.onSuccess { (snapshot, devices) -> update(snapshot, devices) }
                    .onFailure { name.text = "本机名称：暂未获取，请刷新重试" }
            }
        }
    }
}
