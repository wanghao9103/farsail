package app.farsail.android

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
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
        private val dispatch = WakeDispatch()
        internal fun currentLease(context: Context, session: String, epoch: Long, layout: Long) =
            CaptureService.running && ControlSetup.connected(context) && NativeBridge.inputValid(session, epoch, layout)
        fun request(service: ControlService, command: org.json.JSONObject) {
            dispatch.request(SystemClock.elapsedRealtime(),
                authorized = { currentLease(service, command.getString("session"), command.getLong("epoch"), command.getLong("layout")) },
                // ColorOS can reject startActivity while the app is behind the
                // launcher. On Android 8.1-10 use the already-permitted wake API
                // from the control service before requesting any UI.
                wake = { service.wakeLegacyScreen(command) },
                launch = { service.startActivity(Intent(service, WakeActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_HISTORY or Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS)
                    .putExtra("session", command.getString("session")).putExtra("epoch", command.getLong("epoch")).putExtra("layout", command.getLong("layout"))) },
                failed = { android.util.Log.w("FarSailWake", "Authorized wake step unavailable", it) })
        }
    }
    protected open fun authorized() = currentLease(this, intent.getStringExtra("session").orEmpty(), intent.getLongExtra("epoch", -1), intent.getLongExtra("layout", -1))
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
