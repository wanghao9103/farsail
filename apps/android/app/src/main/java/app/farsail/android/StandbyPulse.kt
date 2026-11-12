package app.farsail.android

/** A legacy or broken immediate response must never create a busy network loop. */
internal object StandbyPulse {
    fun supported(waitMs: Long, elapsedMs: Long) = waitMs == 15_000L && elapsedMs >= 10_000L
}
