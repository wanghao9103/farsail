package app.farsail.android

/** Admit fresh projection buffers at 30 FPS, with no catch-up after a slow encode. */
class CaptureCadence {
    companion object { const val TARGET_FPS = 30 }
    private val period = 1_000_000_000L / TARGET_FPS
    private val tolerance = 1_000_000L
    private var next: Long? = null

    fun reset() { next = null }

    fun admit(now: Long): Boolean {
        val due = next
        if (due != null && now < due - tolerance) return false
        next = if (due == null || now - due > tolerance) now + period else due + period
        return true
    }
}
