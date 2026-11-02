package app.farsail.android

import android.app.Activity
import android.os.Bundle
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView

/** An owned non-sensitive target for real remote text input. Only in the test APK. */
class InputFixtureActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val layout = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(40, 120, 40, 40) }
        layout.addView(TextView(this).apply { text = "FarSail synthetic remote input target"; textSize = 22f })
        val input = EditText(this).apply { contentDescription = "remote-input-fixture"; hint = "Waiting for desktop input"; isSingleLine = true }
        layout.addView(input)
        layout.addView(android.widget.Button(this).apply {
            text = "等待远程点击"; contentDescription = "remote-click-target"
            setOnClickListener { text = "远程点击成功" }
        })
        setContentView(layout); input.requestFocus()
    }
}
