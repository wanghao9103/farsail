package app.farsail.android

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.PowerManager
import android.os.Build
import android.provider.Settings
import android.widget.Toast

/** The user grants this once in system UI; no shell whitelist or automatic approval. */
object BackgroundSetup {
    fun colorOs(manufacturer: String = Build.MANUFACTURER, brand: String = Build.BRAND) =
        listOf(manufacturer, brand).any { value -> listOf("oppo", "oplus", "oneplus", "realme").any { value.equals(it, true) } }

    fun captureDetail(sdk: Int = Build.VERSION.SDK_INT) = if (sdk < 30)
        "这台手机使用系统录屏，仍需在手机上确认共享。安卓 10 及以下不支持免重复确认的辅助功能截图；锁屏后可能黑屏或中断，后台设置不能保证锁屏画面和控制。建议共享期间开启防止自动熄屏，中断后解锁并重新确认共享。"
    else "安卓 11 及以上可在「共享这台手机」选择免重复确认的辅助功能截图。需手动授权；厂商后台策略和受保护页面仍可能限制画面，不会自动解除密码锁。"

    fun exempt(context: Context) = context.getSystemService(PowerManager::class.java)
        .isIgnoringBatteryOptimizations(context.packageName)

    fun detail(context: Context) = if (exempt(context))
        "系统电池优化豁免已启用；若仍锁屏掉线，请在应用信息中允许后台活动与自启动。"
    else "尚未允许后台持续运行，系统省电策略可能在锁屏后暂停连接。允许后无需每次重新设置，会增加耗电。"

    /** Try each destination, including fallback failures. Navigation never grants a permission. */
    internal fun launch(intents: List<Intent>, start: (Intent) -> Unit): Boolean {
        for (intent in intents) try { start(intent); return true }
        catch (_: ActivityNotFoundException) { }
        catch (_: SecurityException) { }
        return false
    }

    private fun details(activity: Activity) = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${activity.packageName}"))
    private fun open(activity: Activity, intents: List<Intent>) {
        if (!launch(intents, activity::startActivity))
            Toast.makeText(activity, "无法打开系统页面，请在手机设置中搜索 FarSail，手动管理后台和电池权限。", Toast.LENGTH_LONG).show()
    }

    fun open(activity: Activity) {
        val app = Uri.parse("package:${activity.packageName}")
        val intent = if (exempt(activity)) details(activity)
            else Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, app)
        open(activity, listOf(intent, details(activity), Intent(Settings.ACTION_SETTINGS)))
    }
    fun openAppDetails(activity: Activity) = open(activity, listOf(details(activity), Intent(Settings.ACTION_SETTINGS)))
    fun openPhoneManager(activity: Activity) {
        // Launch installed vendor apps through their public launcher. Private activity names
        // vary by ColorOS version and may be signature protected.
        val managers = listOf("com.coloros.safecenter", "com.oplus.safecenter").mapNotNull {
            activity.packageManager.getLaunchIntentForPackage(it)
        }
        if (managers.isEmpty()) Toast.makeText(activity, "未找到可打开的手机管家，已转到应用详情。请在手机设置中搜索「自启动管理」。", Toast.LENGTH_LONG).show()
        open(activity, managers + listOf(details(activity), Intent(Settings.ACTION_SETTINGS)))
    }
}
