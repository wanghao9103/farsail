package app.farsail.android

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.PowerManager
import android.provider.Settings

/** The user grants this once in system UI; no shell whitelist or automatic approval. */
object BackgroundSetup {
    fun exempt(context: Context) = context.getSystemService(PowerManager::class.java)
        .isIgnoringBatteryOptimizations(context.packageName)

    fun detail(context: Context) = if (exempt(context))
        "系统已允许不受电池优化限制；若仍锁屏掉线，请在应用信息中允许后台活动与自启动。"
    else "尚未允许后台持续运行，系统省电策略可能在锁屏后暂停连接。允许后无需每次重新设置，会增加耗电。"

    fun open(activity: Activity) {
        val app = Uri.parse("package:${activity.packageName}")
        val intent = if (exempt(activity)) Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, app)
            else Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, app)
        try { activity.startActivity(intent) }
        catch (_: ActivityNotFoundException) { activity.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, app)) }
        catch (_: SecurityException) { activity.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, app)) }
    }
}
