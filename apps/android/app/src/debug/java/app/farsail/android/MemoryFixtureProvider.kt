package app.farsail.android

import android.content.ContentProvider
import android.content.ContentValues
import android.database.MatrixCursor
import android.net.Uri
import org.json.JSONObject

/** Read-only, shell-permission-protected state probe. Absent from release builds. */
class MemoryFixtureProvider : ContentProvider() {
    override fun onCreate() = true
    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, sort: String?): android.database.Cursor {
        check(android.os.Build.MODEL.contains("sdk", ignoreCase = true) || android.os.Build.FINGERPRINT.contains("generic")) { "Owned emulator only" }
        require(uri.path == "/state")
        val app = context!!.applicationContext as FarSailApp
        val s = if (app.ready) app.invoke("state") as JSONObject else JSONObject()
        val columns = arrayOf("ready", "signedIn", "bound", "control", "wantedControl", "wantedSharing", "service", "capture", "phase", "pid")
        return MatrixCursor(columns).apply { addRow(arrayOf(app.ready, s.optBoolean("signedIn"), s.optString("deviceId").let { it.isNotBlank() && it != "null" }, s.optBoolean("control"), RemotePreferences.wantsControl(app, s), RemotePreferences.wantsSharing(app, s), ControlService.instance != null, CaptureService.running, CaptureService.phase, android.os.Process.myPid())) }
    }
    override fun getType(uri: Uri) = "vnd.android.cursor.item/vnd.farsail.memory-fixture"
    override fun insert(uri: Uri, values: ContentValues?): Uri? = throw UnsupportedOperationException()
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?) = throw UnsupportedOperationException()
    override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = throw UnsupportedOperationException()
}
