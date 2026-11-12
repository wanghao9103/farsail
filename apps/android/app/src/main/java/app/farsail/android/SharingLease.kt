package app.farsail.android

/** Short, renewable leases belong only to a currently running capture service. */
internal class SharingLease(private val cpu: Lock, private val screen: Lock) : AutoCloseable {
    interface Lock {
        val held: Boolean
        fun acquire(timeoutMs: Long)
        fun release()
    }
    companion object {
        const val TIMEOUT = 90_000L
        const val PERIOD = 30_000L
    }
    private var closed = false
    fun refresh(keepScreen: Boolean) {
        if (closed) return
        cpu.acquire(TIMEOUT)
        if (keepScreen) screen.acquire(TIMEOUT) else if (screen.held) screen.release()
    }
    override fun close() {
        closed = true
        try { if (screen.held) screen.release() } finally { if (cpu.held) cpu.release() }
    }
}
