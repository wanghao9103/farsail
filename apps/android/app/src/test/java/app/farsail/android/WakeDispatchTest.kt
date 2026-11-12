package app.farsail.android

import org.junit.Assert.*
import org.junit.Test

class WakeDispatchTest {
    @Test fun blockedBackgroundWindowDoesNotPreventAuthorizedWake() {
        val events = mutableListOf<String>()
        assertTrue(WakeDispatch().request(0, { true }, { events.add("wake") },
            { events.add("launch"); throw SecurityException("background Activity blocked") }, { events.add("blocked") }))
        assertEquals(listOf("wake", "launch", "blocked"), events)
    }
    @Test fun rejectedProofCannotWakeOrConsumeTheNextValidAttempt() {
        val dispatch = WakeDispatch()
        val events = mutableListOf<String>()
        assertFalse(dispatch.request(0, { false }, { fail("unauthorized wake") }, { fail("unauthorized UI") }, { fail("unexpected exception") }))
        assertTrue(dispatch.request(0, { true }, { events.add("wake") }, { events.add("launch") }, { fail("unexpected exception") }))
        assertEquals(listOf("wake", "launch"), events)
    }
    @Test fun revokedLeaseAfterWakeCannotLaunchAWindow() {
        var active = true
        val events = mutableListOf<String>()
        assertTrue(WakeDispatch().request(0, { active }, { events.add("wake"); active = false },
            { fail("revoked controller launched a window") }, { fail("unexpected exception") }))
        assertEquals(listOf("wake"), events)
    }
    @Test fun repeatedRequestsAreBoundedButAFailedUiDoesNotDisableFutureWake() {
        val dispatch = WakeDispatch()
        var wakes = 0
        val launch = { throw SecurityException("blocked") }
        assertTrue(dispatch.request(0, { true }, { wakes++ }, launch, { }))
        assertFalse(dispatch.request(999, { true }, { wakes++ }, launch, { }))
        assertTrue(dispatch.request(1000, { true }, { wakes++ }, launch, { }))
        assertEquals(2, wakes)
    }
}
