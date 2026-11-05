package app.farsail.android

import kotlin.math.min

/** Display geometry and touch inverse share the same fitted, rotated rectangle. */
data class ViewerGeometry(val viewWidth: Float, val viewHeight: Float, val sourceWidth: Int, val sourceHeight: Int, val turns: Int, val zoom: Float = 1f, val offsetX: Float = 0f, val offsetY: Float = 0f) {
    companion object {
        fun fillZoom(viewWidth: Float, viewHeight: Float, sourceWidth: Int, sourceHeight: Int, turns: Int): Float {
            if (viewWidth <= 0 || viewHeight <= 0 || sourceWidth <= 0 || sourceHeight <= 0) return 1f
            val width = if (turns % 2 == 0) sourceWidth else sourceHeight
            val height = if (turns % 2 == 0) sourceHeight else sourceWidth
            val x = viewWidth / width; val y = viewHeight / height
            return (maxOf(x, y) / minOf(x, y)).coerceIn(1f, 4f)
        }
    }
    val rotatedWidth get() = if (turns % 2 == 0) sourceWidth else sourceHeight
    val rotatedHeight get() = if (turns % 2 == 0) sourceHeight else sourceWidth
    val scale get() = min(viewWidth / rotatedWidth, viewHeight / rotatedHeight) * zoom
    fun point(x: Float, y: Float, clamp: Boolean = false): Pair<Double, Double>? {
        if (viewWidth <= 0 || viewHeight <= 0 || sourceWidth <= 0 || sourceHeight <= 0) return null
        val w = rotatedWidth * scale; val h = rotatedHeight * scale
        val u = ((x - (viewWidth - w) / 2 - offsetX) / w).toDouble()
        val v = ((y - (viewHeight - h) / 2 - offsetY) / h).toDouble()
        if (!u.isFinite() || !v.isFinite() || (!clamp && (u !in 0.0..1.0 || v !in 0.0..1.0))) return null
        val a = u.coerceIn(0.0, 1.0); val b = v.coerceIn(0.0, 1.0)
        return when (turns) { 1 -> b to 1 - a; 2 -> 1 - a to 1 - b; 3 -> 1 - b to a; else -> a to b }
    }
}
