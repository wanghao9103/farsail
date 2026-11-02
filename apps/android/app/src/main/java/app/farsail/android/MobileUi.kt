package app.farsail.android

import android.app.Activity
import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.InputType
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.*

class MobileUi(private val activity: Activity) {
    val ink = Color.rgb(31, 49, 40)
    val muted = Color.rgb(100, 116, 105)
    val green = Color.rgb(49, 92, 75)
    val line = Color.rgb(215, 224, 217)
    val background = Color.rgb(244, 247, 244)
    fun dp(n: Int) = (n * activity.resources.displayMetrics.density).toInt()
    fun column(parent: LinearLayout, padding: Int = 0) = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL; setPadding(dp(padding), dp(padding), dp(padding), dp(padding)); parent.addView(this, LinearLayout.LayoutParams(-1, -2))
    }
    fun gap(parent: LinearLayout, n: Int) { parent.addView(Space(activity), LinearLayout.LayoutParams(1, dp(n))) }
    fun shape(color: Int, radius: Int = 14, border: Boolean = false) = GradientDrawable().apply { setColor(color); cornerRadius = dp(radius).toFloat(); if (border) setStroke(dp(1), line) }
    fun text(parent: LinearLayout, value: String, size: Float = 15f, secondary: Boolean = false, bold: Boolean = false) = TextView(activity).also {
        it.text = value; it.textSize = size; it.setTextColor(if (secondary) muted else ink); it.setLineSpacing(dp(3).toFloat(), 1f)
        if (bold) it.setTypeface(null, Typeface.BOLD)
        parent.addView(it, LinearLayout.LayoutParams(-1, -2))
    }
    fun card(parent: LinearLayout) = column(parent, 20).apply { background = shape(Color.WHITE, 20, true) }
    fun field(parent: LinearLayout, label: String, placeholder: String = "", type: Int = InputType.TYPE_CLASS_TEXT, value: String = ""): EditText {
        val caption = text(parent, label, 14f, bold = true); gap(parent, 8)
        return EditText(activity).also {
            it.id = View.generateViewId(); caption.labelFor = it.id; it.contentDescription = label
            it.hint = placeholder; it.setText(value); it.textSize = 16f; it.setTextColor(ink); it.setHintTextColor(muted)
            it.isSingleLine = true; it.inputType = type; it.imeOptions = EditorInfo.IME_ACTION_NEXT
            it.setPadding(dp(14), dp(12), dp(14), dp(12)); it.background = shape(Color.rgb(248, 250, 248), 12, true)
            it.isSaveEnabled = type and InputType.TYPE_TEXT_VARIATION_PASSWORD == 0
            parent.addView(it, LinearLayout.LayoutParams(-1, dp(54)))
            gap(parent, 20)
        }
    }
    fun button(parent: LinearLayout, label: String, primary: Boolean = true, action: () -> Unit): Button = Button(activity).also {
        it.text = label; it.isAllCaps = false; it.textSize = 16f; it.setTypeface(null, Typeface.BOLD)
        it.setTextColor(if (primary) Color.WHITE else green); it.background = shape(if (primary) green else Color.rgb(232, 240, 234), 13)
        it.setOnClickListener { action() }; it.stateListAnimator = null
        parent.addView(it, LinearLayout.LayoutParams(-1, dp(52))); gap(parent, 12)
    }
    fun link(parent: LinearLayout, label: String, action: () -> Unit) = Button(activity).also {
        it.text = label; it.isAllCaps = false; it.textSize = 14f; it.setTextColor(green); it.background = shape(Color.TRANSPARENT, 10)
        it.setOnClickListener { action() }; it.stateListAnimator = null
        parent.addView(it, LinearLayout.LayoutParams(-1, dp(48)))
    }
    fun row(parent: LinearLayout, title: String, detail: String, action: () -> Unit) {
        val box = column(parent, 16); box.background = shape(Color.WHITE, 14, true)
        text(box, title, 16f, bold = true); gap(box, 5); text(box, detail, 13f, secondary = true)
        box.isClickable = true; box.isFocusable = true; box.contentDescription = title; box.setOnClickListener { action() }
        gap(parent, 12)
    }
}
