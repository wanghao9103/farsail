package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BackgroundSetupTest {
    @Test fun opensSystemExemptionConsentWithoutGrantingIt() {
        val inst = InstrumentationRegistry.getInstrumentation()
        val device = UiDevice.getInstance(inst)
        org.junit.Assume.assumeTrue("Owned emulator only", android.os.Build.MODEL.contains("sdk", true))
        device.wakeUp(); device.pressMenu()
        val context = inst.targetContext
        val before = BackgroundSetup.exempt(context)
        org.junit.Assume.assumeFalse("This case needs an unexempted emulator app", before)
        val activity = inst.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
        try {
            inst.runOnMainSync { BackgroundSetup.open(activity) }
            assertTrue("System battery consent must open", device.wait(Until.hasObject(By.pkg("com.android.settings")), 10000))
            assertTrue("The system must name the app", device.wait(Until.hasObject(By.textContains("FarSail")), 5000))
            assertFalse("Opening the dialog must not grant exemption", BackgroundSetup.exempt(context))
            device.pressBack()
            assertFalse("Dismissing must preserve the user's choice", BackgroundSetup.exempt(context))
        } finally { device.pressBack(); device.pressHome() }
    }
}
