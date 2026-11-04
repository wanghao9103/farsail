package app.farsail.android

import android.content.Context
import org.json.JSONObject

/** Saved intent is scoped to the signed-in server/device, never a capture token. */
object RemotePreferences {
    private var revision = 0L
    private fun prefs(context: Context) = context.getSharedPreferences("remote_access", Context.MODE_PRIVATE)
    private fun scope(state: JSONObject): String? {
        val device = state.optString("deviceId")
        val server = state.optString("server").trimEnd('/')
        return if (state.optBoolean("signedIn") && device.isNotBlank() && device != "null" && server.isNotBlank()) "$server|$device" else null
    }
    private fun remember(context: Context, state: JSONObject, name: String, enabled: Boolean) {
        val p = prefs(context); val key = scope(state)
        val editor = p.edit()
        if (enabled && key != null && p.getString("scope", null) != key) {
            editor.putString("scope", key).putBoolean("control", false).putBoolean("sharing", false)
        }
        editor.putBoolean(name, enabled && key != null).commit()
    }
    private fun wanted(context: Context, state: JSONObject, name: String): Boolean {
        val key = scope(state) ?: return false
        return prefs(context).getString("scope", null) == key && prefs(context).getBoolean(name, false)
    }
    @Synchronized fun ticket() = revision
    @Synchronized fun wantsControl(context: Context, state: JSONObject) = wanted(context, state, "control")
    @Synchronized fun wantsSharing(context: Context, state: JSONObject) = wanted(context, state, "sharing")
    @Synchronized fun control(context: Context, state: JSONObject, enabled: Boolean) {
        revision++
        remember(context, state, "control", enabled)
        NativeBridge.setControl(enabled && scope(state) != null && ControlSetup.connected(context))
    }
    @Synchronized fun restoreControl(context: Context, state: JSONObject, ticket: Long) {
        if (ticket != revision) { android.util.Log.i("FarSailAccess", "Control restore skipped after newer action"); return }
        val connected = ControlSetup.connected(context)
        val enabled = connected && wanted(context, state, "control")
        if (enabled != state.optBoolean("control")) {
            android.util.Log.i("FarSailAccess", "Control restore: connected=$connected wanted=${wanted(context, state, "control")} enabled=$enabled")
            NativeBridge.setControl(enabled)
        }
    }
    @Synchronized fun suspendControl() { revision++; NativeBridge.setControl(false); android.util.Log.i("FarSailAccess", "Control suspended by system") }
    private fun nextConsent(context: Context): Long {
        val old = prefs(context).getLong("consent", 0)
        return if (old == Long.MAX_VALUE) 1 else old + 1
    }
    @Synchronized fun beginConsent(context: Context, state: JSONObject): Long {
        val key = scope(state) ?: return 0
        val p = prefs(context); val edit = p.edit()
        if (p.getString("scope", null) != key) edit.putString("scope", key).putBoolean("control", false).putBoolean("sharing", false)
        val ticket = nextConsent(context)
        // Save the user's choice before leaving for the system permission UI.
        // This does not authorize capture; a fresh successful result is still required.
        edit.putBoolean("sharing", true).putLong("consent", ticket).commit(); return ticket
    }
    @Synchronized fun pendingConsent(context: Context, ticket: Long) = ticket > 0 && prefs(context).getLong("consent", 0) == ticket
    @Synchronized fun abandonConsent(context: Context, ticket: Long): Boolean {
        if (!pendingConsent(context, ticket)) return false
        prefs(context).edit().putLong("consent", nextConsent(context)).commit()
        return true
    }
    @Synchronized fun currentConsent(context: Context, state: JSONObject, ticket: Long): Boolean {
        val key = scope(state) ?: return false
        return pendingConsent(context, ticket) && prefs(context).getString("scope", null) == key
    }
    @Synchronized fun sharing(context: Context, state: JSONObject, ticket: Long): Boolean {
        if (!currentConsent(context, state, ticket)) return false
        prefs(context).edit().putBoolean("sharing", true).putLong("consent", nextConsent(context)).commit()
        return true
    }
    @Synchronized fun stopSharing(context: Context) {
        prefs(context).edit().putBoolean("sharing", false).putLong("consent", nextConsent(context)).commit()
    }
    fun autoRestore(context: Context) = prefs(context).getBoolean("autoRestore", true)
    fun autoRestore(context: Context, enabled: Boolean) { prefs(context).edit().putBoolean("autoRestore", enabled).apply() }
    @Synchronized fun clear(context: Context) {
        revision++
        prefs(context).edit().remove("scope").remove("control").remove("sharing").putLong("consent", nextConsent(context)).commit()
        NativeBridge.setControl(false)
    }
}
