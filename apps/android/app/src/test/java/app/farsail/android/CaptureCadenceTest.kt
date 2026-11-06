package app.farsail.android

import org.junit.Assert.*
import org.junit.Test

class CaptureCadenceTest {
    @Test fun sixtyHzSourceWithClockJitterKeepsThirtyFps() {
        val cadence = CaptureCadence()
        val accepted = (0 until 600).map { frame ->
            frame * 1_000_000_000L / 60 + if (frame % 4 == 2) -250_000L else 0L
        }.filter { cadence.admit(it) }
        assertEquals(300, accepted.size)
        assertTrue(accepted.zipWithNext().all { (a, b) -> b - a >= 32_000_000L })
    }

    @Test fun slowEncodeNeverProducesACatchUpBurst() {
        val cadence = CaptureCadence()
        assertTrue(cadence.admit(0))
        assertTrue(cadence.admit(200_000_000L))
        assertFalse(cadence.admit(201_000_000L))
        assertFalse(cadence.admit(216_000_000L))
        assertTrue(cadence.admit(233_333_333L))
    }

    @Test fun resizedSurfaceCanPublishItsFirstFreshFrameImmediately() {
        val cadence = CaptureCadence()
        assertTrue(cadence.admit(0))
        assertFalse(cadence.admit(10_000_000L))
        cadence.reset()
        assertTrue(cadence.admit(10_000_000L))
    }
}
