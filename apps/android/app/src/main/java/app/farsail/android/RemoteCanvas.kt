package app.farsail.android

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.view.MotionEvent
import android.view.View
import android.view.ScaleGestureDetector
import org.json.JSONObject

class RemoteCanvas(context: Context, private val send: (JSONObject?) -> Unit) : View(context) {
    data class Frame(val bitmap: Bitmap, val display: Int, val layout: Long, val sequence: Long)
    private val paint = Paint(Paint.FILTER_BITMAP_FLAG)
    private var frame: Frame? = null
    private var touching = false
    private var pointer = -1
    private var lastX = 0f
    private var lastY = 0f
    private var lastPoint = .5 to .5
    private var lastMove = 0L
    private var zoom = 1f
    private var offsetX = 0f
    private var offsetY = 0f
    private var navigating = false
    private var gestureX = 0f
    private var gestureY = 0f
    private val scaleGesture = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScaleBegin(detector: ScaleGestureDetector): Boolean { release(); return true }
        override fun onScale(detector: ScaleGestureDetector): Boolean {
            val next = (zoom * detector.scaleFactor).coerceIn(1f, 4f)
            val ratio = next / zoom
            offsetX = detector.focusX - width / 2f - (detector.focusX - width / 2f - offsetX) * ratio
            offsetY = detector.focusY - height / 2f - (detector.focusY - height / 2f - offsetY) * ratio
            zoom = next; boundPan(); invalidate(); return true
        }
    })
    var turns = 0
        private set
    var scrolling = false
        private set
    var control = false
        set(value) { if (field && !value) release(); field = value }
    init { setBackgroundColor(Color.rgb(16, 27, 32)); contentDescription = "远程画面"; isFocusable = true }
    fun show(next: Frame) {
        if (frame?.layout != next.layout || frame?.display != next.display) fit()
        frame = next; invalidate()
    }
    fun clear() { fit(); frame = null; invalidate() }
    fun fit() { release(); zoom = 1f; offsetX = 0f; offsetY = 0f; invalidate() }
    fun rotate() { fit(); turns = (turns + 1) % 4; invalidate() }
    fun scrollMode(): Boolean { release(); scrolling = !scrolling; return scrolling }
    fun release() { if (touching) send(null); touching = false; pointer = -1 }
    private fun geometry() = frame?.let { ViewerGeometry(width.toFloat(), height.toFloat(), it.bitmap.width, it.bitmap.height, turns, zoom, offsetX, offsetY) }
    private fun boundPan() {
        val g = geometry() ?: return
        val x = ((g.rotatedWidth * g.scale - width) / 2).coerceAtLeast(0f)
        val y = ((g.rotatedHeight * g.scale - height) / 2).coerceAtLeast(0f)
        offsetX = offsetX.coerceIn(-x, x); offsetY = offsetY.coerceIn(-y, y)
    }
    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val image = frame ?: return; val g = geometry() ?: return
        canvas.save(); canvas.translate(width / 2f + offsetX, height / 2f + offsetY); canvas.rotate(turns * 90f); canvas.scale(g.scale, g.scale)
        canvas.drawBitmap(image.bitmap, -image.bitmap.width / 2f, -image.bitmap.height / 2f, paint); canvas.restore()
    }
    private fun event(kind: String, point: Pair<Double,Double> = lastPoint): JSONObject? {
        val image = frame ?: return null
        return JSONObject().put("kind", kind).put("display", image.display).put("layout", image.layout).put("x", point.first).put("y", point.second)
    }
    fun rightClick() {
        if (!control) return
        event("button")?.let { send(JSONObject(it.toString()).put("button", "right").put("down", true)); send(it.put("button", "right").put("down", false)) }
    }
    override fun onTouchEvent(e: MotionEvent): Boolean {
        if (frame == null) return false
        scaleGesture.onTouchEvent(e)
        if (e.pointerCount > 1) {
            release(); parent?.requestDisallowInterceptTouchEvent(true)
            val x = (e.getX(0) + e.getX(1)) / 2
            val y = (e.getY(0) + e.getY(1)) / 2
            if (navigating && e.actionMasked == MotionEvent.ACTION_MOVE) {
                offsetX += x - gestureX; offsetY += y - gestureY; boundPan(); invalidate()
            }
            navigating = true; gestureX = x; gestureY = y
            return true
        }
        // The last finger of a local pinch never becomes a remote click.
        if (navigating) {
            if (e.actionMasked == MotionEvent.ACTION_UP || e.actionMasked == MotionEvent.ACTION_CANCEL) navigating = false
            return true
        }
        if (!control) return true
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val p = geometry()?.point(e.x, e.y) ?: return false
                parent?.requestDisallowInterceptTouchEvent(true)
                touching = true; pointer = e.getPointerId(0); lastX = e.x; lastY = e.y; lastPoint = p
                if (!scrolling) event("button", p)?.let { send(it.put("button", "left").put("down", true)) }
            }
            MotionEvent.ACTION_MOVE -> {
                if (!touching || e.getPointerId(0) != pointer) return true
                val p = geometry()?.point(e.x, e.y, true) ?: return true
                if (e.eventTime - lastMove >= 32) {
                    lastMove = e.eventTime
                    if (scrolling) {
                        val h = ((lastX - e.x) * 2).toInt().coerceIn(-1200, 1200)
                        val v = ((e.y - lastY) * 2).toInt().coerceIn(-1200, 1200)
                        val axes = when (turns) { 1 -> -v to h; 2 -> -h to -v; 3 -> v to -h; else -> h to v }
                        event("wheel", p)?.let { send(it.put("horizontal", axes.first).put("vertical", axes.second)) }
                        lastX = e.x; lastY = e.y
                    } else event("move", p)?.let(send)
                }
                lastPoint = p
            }
            MotionEvent.ACTION_UP -> {
                if (touching && e.getPointerId(0) == pointer && !scrolling) {
                    val p = geometry()?.point(e.x, e.y, true) ?: lastPoint
                    lastPoint = p; event("button", p)?.let { send(it.put("button", "left").put("down", false)) }
                }
                touching = false; pointer = -1; performClick()
            }
            MotionEvent.ACTION_CANCEL -> release()
        }
        return true
    }
    override fun performClick(): Boolean { super.performClick(); return true }
    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) { release(); boundPan(); super.onSizeChanged(w,h,oldw,oldh) }
}
