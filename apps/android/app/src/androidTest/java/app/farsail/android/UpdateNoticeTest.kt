package app.farsail.android

import android.content.Intent
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Owned emulator, injected presentation states only; no package installation. */
@RunWith(AndroidJUnit4::class)
class UpdateNoticeTest {
    @Test fun homeReminderKeepsOneActionAndRespectsSavedDownloadChoices() {
        org.junit.Assume.assumeTrue(android.os.Build.MODEL.contains("sdk", true) || android.os.Build.FINGERPRINT.contains("generic"))
        val inst = InstrumentationRegistry.getInstrumentation()
        val context = inst.targetContext
        val prefs = context.getSharedPreferences("updates", 0)
        val saved = prefs.all.toMap()
        prefs.edit().clear().putBoolean("autoCheck", false).commit()
        val activity = inst.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
        try {
            inst.runOnMainSync {
                val app = context.applicationContext as FarSailApp
                assertTrue(app.updates.autoDownload); assertTrue(app.updates.wifiOnly)
                app.updates.preferences(false, false, false)
                val restored = Updates(app)
                assertFalse(restored.autoCheck); assertFalse(restored.autoDownload); assertFalse(restored.wifiOnly)
                var actions = 0
                val reminder = UpdateNoticeView(activity) { actions++ }
                val root = LinearLayout(activity).apply { orientation = LinearLayout.VERTICAL; addView(reminder) }
                activity.setContentView(root)
                reminder.render(UpdateState("upToDate")); assertEquals(View.GONE, reminder.visibility)
                reminder.render(UpdateState("available", version = "99.0.0")); assertEquals(View.VISIBLE, reminder.visibility)
                val button = (0 until reminder.childCount).map(reminder::getChildAt).filterIsInstance<Button>().single()
                assertEquals("下载更新", button.text.toString()); button.performClick(); assertEquals(1, actions)
                reminder.render(UpdateState("downloading", version = "99.0.0", progress = 37))
                assertEquals("查看下载进度", button.text.toString()); button.performClick(); assertEquals(2, actions)
                reminder.render(UpdateState("ready", version = "99.0.0"))
                assertEquals("安装更新", button.text.toString()); button.performClick(); assertEquals(3, actions)
                reminder.render(UpdateState("error")); assertEquals(View.GONE, reminder.visibility)
            }
        } finally {
            inst.runOnMainSync { activity.finish() }
            val edit = prefs.edit().clear()
            saved.forEach { (key, value) -> when (value) { is Boolean -> edit.putBoolean(key, value); is Long -> edit.putLong(key, value); is Int -> edit.putInt(key, value); is String -> edit.putString(key, value) } }
            edit.commit()
        }
    }
}
