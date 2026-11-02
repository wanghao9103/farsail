package app.farsail.android

import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.os.PowerManager

/** Scoped to the explicit projection service, including when its Activity is in the background. */
class SharingPower(private val context: Context) : AutoCloseable {
    companion object {
        private const val LIMIT = 8 * 60 * 60 * 1000L
        fun enabled(context: Context) = context.getSharedPreferences("sharing", Context.MODE_PRIVATE).getBoolean("keepScreenOn", true)
        fun preference(context: Context, value: Boolean) {
            context.getSharedPreferences("sharing", Context.MODE_PRIVATE).edit().putBoolean("keepScreenOn", value).apply()
            if (CaptureService.running) context.startService(Intent(context, CaptureService::class.java).setAction(CaptureService.POWER))
        }
    }
    private val manager = context.getSystemService(PowerManager::class.java)
    private var cpu: PowerManager.WakeLock? = null
    private var screen: PowerManager.WakeLock? = null
    fun start() {
        cpu = manager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "FarSail:Sharing").apply { acquire(LIMIT) }
        keepScreen(enabled(context))
    }
    @Suppress("DEPRECATION")
    fun keepScreen(enabled: Boolean) {
        if (!enabled) { screen?.let { if (it.isHeld) it.release() }; screen = null; return }
        // A foreground service must cover other apps too. FLAG_KEEP_SCREEN_ON
        // only covers our visible Activity. Do not wake or unlock the device.
        if (screen == null && manager.isInteractive && !context.getSystemService(KeyguardManager::class.java).isKeyguardLocked) {
            screen = manager.newWakeLock(PowerManager.SCREEN_DIM_WAKE_LOCK, "FarSail:SharingScreen").apply { acquire(LIMIT) }
        }
    }
    override fun close() {
        screen?.let { if (it.isHeld) it.release() }; screen = null
        cpu?.let { if (it.isHeld) it.release() }; cpu = null
    }
}
