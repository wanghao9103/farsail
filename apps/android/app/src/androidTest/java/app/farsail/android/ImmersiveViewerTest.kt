package app.farsail.android

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.view.View
import android.view.ViewGroup
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Layout-only synthetic canvas test; no account, endpoint or remote input. */
@RunWith(AndroidJUnit4::class)
class ImmersiveViewerTest {
    @Test fun canvasKeepsFullAreaWhenFloatingToolsAreOpenedOrCollapsed() {
        org.junit.Assume.assumeTrue("Owned emulator only", android.os.Build.MODEL.contains("sdk", true) || android.os.Build.FINGERPRINT.contains("generic"))
        val inst = InstrumentationRegistry.getInstrumentation()
        val context = inst.targetContext
        val device = UiDevice.getInstance(inst)
        device.wakeUp(); device.pressMenu()
        val activity = inst.startActivitySync(Intent(context, ViewerActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as ViewerActivity
        fun canvas(view: View): RemoteCanvas? = if (view is RemoteCanvas) view else if (view is ViewGroup) (0 until view.childCount).firstNotNullOfOrNull { canvas(view.getChildAt(it)) } else null
        try {
            inst.waitForIdleSync()
            if (device.wait(Until.hasObject(By.res("android:id/immersive_cling_title")), 1500)) {
                device.findObject(By.res("android:id/ok"))?.click()
            }
            device.waitForIdle()
            Thread.sleep(500)
            lateinit var remote: RemoteCanvas
            var initialWidth = 0; var initialHeight = 0
            inst.runOnMainSync {
                remote = canvas(activity.window.decorView)!!
                val bitmap = Bitmap.createBitmap(360, 720, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.rgb(36, 140, 116)) }
                remote.show(RemoteCanvas.Frame(bitmap, 1, 1, 1))
                initialWidth = remote.width; initialHeight = remote.height
                val root = remote.parent as View
                assertEquals(root.width, initialWidth); assertEquals(root.height, initialHeight)
            }
            device.takeScreenshot(java.io.File(context.filesDir, "viewer-before-tools.png"))
            val collapse = device.wait(Until.findObject(By.desc("收起远程工具")), 10000)
            assertNotNull("Floating tools must be in the resumed viewer", collapse)
            collapse!!.click()
            assertNotNull(device.wait(Until.findObject(By.desc("显示远程工具")), 3000))
            inst.runOnMainSync { assertEquals(initialWidth, remote.width); assertEquals(initialHeight, remote.height) }
            device.takeScreenshot(java.io.File(context.filesDir, "viewer-immersive-collapsed.png"))
            device.findObject(By.desc("显示远程工具")).click()
            assertNotNull(device.wait(Until.findObject(By.desc("收起远程工具")), 3000))
            inst.runOnMainSync { assertEquals(initialWidth, remote.width); assertEquals(initialHeight, remote.height) }
            device.takeScreenshot(java.io.File(context.filesDir, "viewer-immersive-tools.png"))
            inst.runOnMainSync { remote.fill(); remote.zoomBy(1.5f); remote.zoomBy(1f / 1.5f); remote.fit(); assertEquals(initialWidth, remote.width); assertEquals(initialHeight, remote.height) }
        } finally { device.dumpWindowHierarchy(java.io.File(context.filesDir, "viewer-layout.xml")); inst.runOnMainSync { activity.finish() } }
    }
}
