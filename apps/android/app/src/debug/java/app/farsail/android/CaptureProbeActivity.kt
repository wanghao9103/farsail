package app.farsail.android

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.view.View

/** Synthetic pixels for owned-emulator feasibility tests; absent from release. */
class CaptureProbeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(View(this).apply { setBackgroundColor(Color.RED) })
    }
}

/** Isolates window wake mechanics from native session authorization in the API probe. */
class WakeProbeActivity : WakeActivity() {
    override fun authorized() = android.os.Build.MODEL.contains("sdk", true)
}
