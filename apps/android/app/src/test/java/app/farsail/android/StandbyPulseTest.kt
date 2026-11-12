package app.farsail.android

import org.junit.Assert.*
import org.junit.Test

class StandbyPulseTest {
    @Test fun aServerDelayedReplyCanDriveTheNextRequest() {
        assertTrue(StandbyPulse.supported(15000, 15020))
        assertTrue(StandbyPulse.supported(15000, 18000))
    }
    @Test fun oldOrMisconfiguredServersCannotCauseABusyLoop() {
        assertFalse(StandbyPulse.supported(0, 100))
        assertFalse(StandbyPulse.supported(15000, 100))
        assertFalse(StandbyPulse.supported(-1, 15000))
        assertFalse(StandbyPulse.supported(60000, 15000))
    }
}
