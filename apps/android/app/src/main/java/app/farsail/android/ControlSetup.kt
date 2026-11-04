package app.farsail.android

import android.app.Activity
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.view.accessibility.AccessibilityManager

/** Navigation only. System consent remains in Settings; no secure setting is written. */
object ControlSetup {
    fun enabled(context: Context): Boolean = try {
        context.getSystemService(AccessibilityManager::class.java)
            .getEnabledAccessibilityServiceList(-1).any {
                val service = it.resolveInfo.serviceInfo
                ComponentName(service.packageName, service.name) == ComponentName(context, ControlService::class.java)
            }
    } catch (_: Exception) { false }
    fun connected(context: Context) = ControlService.instance != null && enabled(context)
    fun status(context: Context, state: org.json.JSONObject) = ControlStatus(
        RemotePreferences.wantsControl(context, state), enabled(context), ControlService.instance != null,
        state.optBoolean("control"), state.optBoolean("sharing") && CaptureService.running && CaptureService.phase == "sharing", state.optBoolean("remoteWatch"),
    )
    fun open(activity: Activity): Boolean {
        val component = ComponentName(activity, ControlService::class.java)
        val detail = Intent("android.settings.ACCESSIBILITY_DETAILS_SETTINGS").putExtra(Intent.EXTRA_COMPONENT_NAME, component.flattenToString())
        try { activity.startActivity(detail); return true } catch (_: android.content.ActivityNotFoundException) { } catch (_: SecurityException) { }
        val arguments = android.os.Bundle().apply { putString(":settings:fragment_args_key", component.flattenToString()) }
        val list = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)
            .putExtra(":settings:fragment_args_key", component.flattenToString())
            .putExtra(":settings:show_fragment_args", arguments)
        android.widget.Toast.makeText(activity, "点击已定位的「FarSail 远程控制」，打开使用开关。", android.widget.Toast.LENGTH_LONG).show()
        return try { activity.startActivity(list); false } catch (_: android.content.ActivityNotFoundException) { false } catch (_: SecurityException) { false }
    }
}
