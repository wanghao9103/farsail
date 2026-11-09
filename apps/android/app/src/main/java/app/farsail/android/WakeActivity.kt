package app.farsail.android

import android.app.Activity
import android.app.KeyguardManager
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock

/** Wake only for a live approved controller; never dismiss a credential lock. */
open class WakeActivity : Activity() {
    private var screenWake: PowerManager.WakeLock? = null
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
    protected open fun authorized() = CaptureService.running && ControlSetup.connected(this) &&
        NativeBridge.inputValid(intent.getStringExtra("session").orEmpty(), intent.getLongExtra("epoch", -1), intent.getLongExtra("layout", -1))
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT < 27 || !authorized()) { finish(); return }
        setShowWhenLocked(true)
        setTurnScreenOn(true)
        // Android 10 can pause this translucent Activity again before its
        // turnScreenOn window becomes visible. Use the legacy, bounded wake
        // lock on these releases, only AFTER validating the native lease.
        if (Build.VERSION.SDK_INT <= 29) wakeLegacyScreen()
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE)
        Handler(Looper.getMainLooper()).postDelayed({ finish() }, 500)
    }
    @Suppress("DEPRECATION")
    private fun wakeLegacyScreen() {
        screenWake = getSystemService(PowerManager::class.java).newWakeLock(
            PowerManager.SCREEN_BRIGHT_WAKE_LOCK or PowerManager.ACQUIRE_CAUSES_WAKEUP, "FarSail:RemoteWake"
        ).apply { acquire(1500) }
    }
    override fun onResume() {
        super.onResume()
        if (isFinishing || !authorized()) { finish(); return }
        val keyguard = getSystemService(KeyguardManager::class.java)
        if (!keyguard.isDeviceSecure && keyguard.isKeyguardLocked) keyguard.requestDismissKeyguard(this, null)
    }
    override fun onDestroy() {
        screenWake?.let { if (it.isHeld) it.release() }; screenWake = null
        super.onDestroy()
    }
}
