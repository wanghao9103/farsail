package app.farsail.android

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock

/** Wake on an explicit, live controller action. Never dismisses the keyguard. */
open class WakeActivity : Activity() {
    companion object {
        private var lastAttempt = 0L
        fun request(service: ControlService, command: org.json.JSONObject) {
            val now = SystemClock.elapsedRealtime()
            if (now - lastAttempt < 1000) return
            lastAttempt = now
            try {
                service.startActivity(Intent(service, WakeActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_HISTORY or Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS)
                    .putExtra("session", command.getString("session")).putExtra("epoch", command.getLong("epoch")).putExtra("layout", command.getLong("layout")))
            } catch (_: Exception) { android.util.Log.w("FarSailWake", "Screen wake request unavailable") }
        }
    }
    protected open fun authorized() = CaptureService.running && CaptureService.accessibilityMode && ControlSetup.connected(this) &&
        NativeBridge.inputValid(intent.getStringExtra("session").orEmpty(), intent.getLongExtra("epoch", -1), intent.getLongExtra("layout", -1))
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT < 27 || !authorized()) { finish(); return }
        setShowWhenLocked(true)
        setTurnScreenOn(true)
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE)
        Handler(Looper.getMainLooper()).postDelayed({ finish() }, 500)
    }
}
