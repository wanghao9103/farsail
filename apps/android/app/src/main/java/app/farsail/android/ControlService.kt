package app.farsail.android

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
import android.graphics.Path
import android.os.*
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import org.json.JSONArray
import org.json.JSONObject

/** Input is consumed on the main thread; every delayed action rechecks its live native lease. */
class ControlService : AccessibilityService() {
    companion object {
        @Volatile var instance: ControlService? = null
            private set
        fun cancel() { instance?.clearGesture() }
    }
    private val handler = Handler(Looper.getMainLooper())
    private var pointer: MutableList<Pair<Float, Float>>? = null
    private var pointerAt = 0L
    private var pointerOwner = ""
    private var pointerLayout = 0L
    private var shift = false
    private var ctrl = false
    private var gestureBusy = false
    private val gestures = java.util.ArrayDeque<Pair<JSONObject, GestureDescription>>()
    override fun onServiceConnected() {
        instance = this
        (application as FarSailApp).restoreControl()
        handler.post(poll)
        (application as FarSailApp).observer?.invoke()
    }
    private val poll = object : Runnable {
        override fun run() {
            val app = application as FarSailApp
            if (app.ready) {
                try {
                    val result = app.unwrap(NativeBridge.drain()) as JSONArray
                    for (i in 0 until result.length()) {
                        val command = result.getJSONObject(i)
                        if (valid(command)) apply(command) else clearGesture()
                    }
                    if (pointerOwner.isNotEmpty() && !NativeBridge.inputValid(pointerOwner, pointerEpoch, pointerLayout)) clearGesture()
                } catch (_: Exception) { clearGesture() }
            }
            handler.postDelayed(this, 16)
        }
    }
    private var pointerEpoch = 0L
    private fun valid(c: JSONObject): Boolean {
        if (!CaptureService.running) return false
        val (w, h) = dimensions(this)
        return w == CaptureService.screenWidth && h == CaptureService.screenHeight && NativeBridge.inputValid(c.getString("session"), c.getLong("epoch"), c.getLong("layout"))
    }
    private fun point(v: JSONObject): Pair<Float, Float> =
        (v.getDouble("x") * (CaptureService.screenWidth - 1)).toFloat() to (v.getDouble("y") * (CaptureService.screenHeight - 1)).toFloat()
    private fun apply(c: JSONObject) {
        val v = c.getJSONObject("input")
        if (CaptureService.accessibilityMode && !getSystemService(PowerManager::class.java).isInteractive) {
            // The caller has already checked the native session/epoch/layout lease.
            // Consume the wake action so its coordinates cannot hit a newly visible UI.
            if (v.getString("kind") !in listOf("move", "release")) WakeActivity.request(this, c)
            clearGesture(); return
        }
        when (v.getString("kind")) {
            "release" -> clearGesture()
            "move" -> pointer?.let { if (it.size < 128) it.add(point(v)) }
            "button" -> {
                if (v.getString("button") == "right") { if (!v.getBoolean("down")) performGlobalAction(GLOBAL_ACTION_BACK); return }
                if (v.getBoolean("down")) {
                    pointer = mutableListOf(point(v)); pointerAt = SystemClock.elapsedRealtime()
                    pointerOwner = c.getString("session"); pointerEpoch = c.getLong("epoch"); pointerLayout = c.getLong("layout")
                } else {
                    val points = pointer ?: return
                    if (pointerOwner != c.getString("session") || pointerEpoch != c.getLong("epoch") || pointerLayout != c.getLong("layout")) { clearGesture(); return }
                    points.add(point(v)); pointer = null; pointerOwner = ""
                    val elapsed = (SystemClock.elapsedRealtime() - pointerAt).coerceIn(50, 1500)
                    val path = Path().apply {
                        moveTo(points.first().first, points.first().second)
                        points.drop(1).forEach { (x, y) -> lineTo(x, y) }
                    }
                    gesture(c, path, elapsed)
                }
            }
            "wheel" -> {
                val (x, y) = point(v)
                val vertical = v.getInt("vertical"); val horizontal = v.getInt("horizontal")
                if (vertical == 0 && horizontal == 0) return
                val distance = minOf(CaptureService.screenHeight * 0.35f, 450f)
                val endY = (y + if (vertical > 0) distance else if (vertical < 0) -distance else 0f).coerceIn(1f, CaptureService.screenHeight - 2f)
                val endX = (x + if (horizontal > 0) -distance else if (horizontal < 0) distance else 0f).coerceIn(1f, CaptureService.screenWidth - 2f)
                gesture(c, Path().apply { moveTo(x, y); lineTo(endX, endY) }, 250)
            }
            "text" -> edit { _, text, start, end -> text.replaceRange(start, end, v.getString("text")) }
            "key" -> key(v)
        }
    }
    private fun gesture(c: JSONObject, path: Path, duration: Long) {
        if (gestures.size >= 16) { RemotePreferences.suspendControl(); clearGesture(); return }
        gestures.add(c to GestureDescription.Builder().addStroke(GestureDescription.StrokeDescription(path, 0, duration)).build())
        nextGesture()
    }
    private fun nextGesture() {
        if (gestureBusy) return
        while (gestures.isNotEmpty()) {
            val (c, gesture) = gestures.removeFirst()
            if (!valid(c)) continue
            gestureBusy = true
            val accepted = dispatchGesture(gesture, object : GestureResultCallback() {
                override fun onCompleted(gestureDescription: GestureDescription?) { finish() }
                override fun onCancelled(gestureDescription: GestureDescription?) { finish() }
                private fun finish() { gestureBusy = false; nextGesture() }
            }, handler)
            if (!accepted) { gestureBusy = false; continue }
            break
        }
    }
    private fun clearGesture() {
        pointer = null; pointerOwner = ""; gestures.clear(); shift = false; ctrl = false
        // Android cannot recall a gesture already accepted by the system. Do not
        // inject a dummy tap to cancel it. Clear all queued work; the bounded
        // in-flight gesture completes within 1.5 seconds.

    }
    private fun key(v: JSONObject) {
        val vk = v.getInt("vk"); val down = v.getBoolean("down")
        if (vk == 16) { shift = down; return }; if (vk == 17) { ctrl = down; return }
        if (!down) return
        when (vk) {
            27, 166 -> performGlobalAction(GLOBAL_ACTION_BACK)
            172 -> performGlobalAction(GLOBAL_ACTION_HOME)
            93 -> performGlobalAction(GLOBAL_ACTION_RECENTS)
            13 -> if (Build.VERSION.SDK_INT >= 30) focused { it.performAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id) }
            8 -> edit { _, text, start, end -> text.removeRange(if (start == end) (start - 1).coerceAtLeast(0) else start, end) }
            46 -> edit { _, text, start, end -> text.removeRange(start, if (start == end) (end + 1).coerceAtMost(text.length) else end) }
            65 -> if (ctrl) focused { node -> node.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, Bundle().apply {
                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, 0); putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, node.text?.length ?: 0)
            }) } else type(if (shift) "A" else "a")
            in 66..90 -> type((if (shift) vk else vk + 32).toChar().toString())
            in 48..57 -> if (!ctrl) type(vk.toChar().toString())
            32 -> type(" ")
        }
    }
    private fun type(text: String) { if (!ctrl) edit { _, old, start, end -> old.replaceRange(start, end, text) } }
    @Suppress("DEPRECATION") private fun focused(action: (AccessibilityNodeInfo) -> Unit) {
        val root = rootInActiveWindow ?: return
        val node = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
        try { if (node != null && node.isEditable && !node.isPassword) action(node) }
        finally { node?.recycle(); root.recycle() }
    }
    private fun edit(change: (AccessibilityNodeInfo, String, Int, Int) -> String) {
        focused { node ->
            val text = if (node.isShowingHintText) "" else node.text?.toString() ?: ""
            val start = node.textSelectionStart.coerceIn(0, text.length)
            val end = node.textSelectionEnd.coerceIn(start, text.length)
            val next = change(node, text, start, end)
            if (next.length <= 32768) node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, Bundle().apply {
                putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, next)
            })
        }
    }
    override fun onAccessibilityEvent(event: AccessibilityEvent?) { }
    override fun onInterrupt() {
        clearGesture()
        // This callback interrupts accessibility feedback, not the OS grant.
        // Revocation is enforced by the settings observer and actual unbinding.
        if (!ControlSetup.connected(this)) {
            RemotePreferences.suspendControl()
            CaptureService.accessibilityRevoked(this)
        }
        val app = application as FarSailApp
        app.io.execute { runCatching { app.refresh() } }
    }
    override fun onUnbind(intent: android.content.Intent?): Boolean {
        disconnected()
        return super.onUnbind(intent)
    }
    private fun disconnected() {
        if (instance !== this) return
        instance = null; RemotePreferences.suspendControl(); clearGesture(); handler.removeCallbacks(poll)
        CaptureService.accessibilityRevoked(this)
        (application as FarSailApp).restoreControl()
    }
    override fun onDestroy() {
        disconnected(); super.onDestroy()
    }
}
