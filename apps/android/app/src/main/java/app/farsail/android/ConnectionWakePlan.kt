package app.farsail.android

/** One outstanding monotonic deadline; regular heartbeats must not postpone it. */
internal class ConnectionWakePlan(private val schedule: (Long) -> Unit, private val cancel: () -> Unit) : AutoCloseable {
    companion object { const val PERIOD = 25_000L }
    var nextAt = 0L
        private set
    private var closed = false
    fun ensure(now: Long, eligible: Boolean) {
        if (closed) return
        if (!eligible) {
            if (nextAt != 0L) { cancel(); nextAt = 0 }
        } else if (nextAt == 0L) {
            val deadline = now + PERIOD
            schedule(deadline)
            nextAt = deadline
        }
    }
    fun delivered(): Boolean {
        if (closed || nextAt == 0L) return false
        nextAt = 0
        return true
    }
    override fun close() { closed = true; nextAt = 0; cancel() }
}
