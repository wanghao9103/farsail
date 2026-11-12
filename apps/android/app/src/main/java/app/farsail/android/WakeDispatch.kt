package app.farsail.android

/** A blocked background Activity must not prevent an authorized screen wake. */
class WakeDispatch {
    private var lastAttempt: Long? = null
    fun request(now: Long, authorized: () -> Boolean, wake: () -> Unit, launch: () -> Unit, failed: (Exception) -> Unit): Boolean {
        if (!authorized()) return false
        val previous = lastAttempt
        if (previous != null && now >= previous && now - previous < 1000) return false
        lastAttempt = now
        try { wake() } catch (e: Exception) { failed(e) }
        // Waking can change display state or outlive a lease. Do not launch an
        // old controller's window after its authorization has changed.
        if (authorized()) try { launch() } catch (e: Exception) { failed(e) }
        return true
    }
}
