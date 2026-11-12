package app.farsail.android

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.os.PowerManager
import android.os.SystemClock
import org.json.JSONArray
import org.json.JSONObject

/** Bounded local power/heartbeat evidence, without account, host or network identifiers. */
internal class ConnectionJournal(private val context: Context) {
    private val saved = context.getSharedPreferences("connection_diagnostics", Context.MODE_PRIVATE)
    private var lastCheckpoint = 0L
    private var lastBeat = saved.getLong("lastHeartbeatWall", 0)
    private var failed = false
    private val history = runCatching { JSONArray(saved.getString("events", "[]")) }.getOrDefault(JSONArray())
    @Synchronized fun record(event: String) {
        val power = context.getSystemService(PowerManager::class.java)
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val capabilities = connectivity.getNetworkCapabilities(connectivity.activeNetwork)
        val item = JSONObject().put("at", System.currentTimeMillis()).put("event", event)
            .put("charging", context.getSystemService(BatteryManager::class.java).isCharging)
            .put("interactive", power.isInteractive).put("idle", power.isDeviceIdleMode)
            .put("powerSave", power.isPowerSaveMode).put("exempt", BackgroundSetup.exempt(context))
            .put("network", capabilities?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true)
        history.put(item)
        while (history.length() > 24) history.remove(0)
        saved.edit().putString("events", history.toString()).apply()
    }
    @Synchronized fun heartbeat() {
        val wall = System.currentTimeMillis()
        val gap = if (lastBeat > 0) wall - lastBeat else 0
        if (gap > 45_000) record("heartbeat_wall_gap_ms=$gap")
        else if (failed) record("heartbeat_recovered")
        failed = false; lastBeat = wall
        val now = SystemClock.elapsedRealtime()
        if (lastCheckpoint == 0L || now - lastCheckpoint >= 60_000 || gap > 45_000) {
            saved.edit().putLong("lastHeartbeatWall", wall).apply()
            lastCheckpoint = now
        }
    }
    @Synchronized fun failure(code: String) { if (!failed) record("heartbeat_failure=$code"); failed = true }
    @Synchronized fun dump(writer: java.io.PrintWriter) {
        writer.println("lastHeartbeatWall=$lastBeat powerHistory=$history")
    }
}
