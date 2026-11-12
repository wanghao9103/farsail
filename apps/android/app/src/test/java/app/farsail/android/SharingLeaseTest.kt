package app.farsail.android

import org.junit.Assert.*
import org.junit.Test

class SharingLeaseTest {
    private class Clock { var now = 0L }
    private class Lock(private val clock: Clock) : SharingLease.Lock {
        var expires = 0L
        var acquisitions = 0
        override val held get() = expires > clock.now
        override fun acquire(timeoutMs: Long) { expires = clock.now + timeoutMs; acquisitions++ }
        override fun release() { expires = 0 }
    }
    @Test fun continuingCaptureStaysHeldBeyondEightHours() {
        val clock = Clock(); val cpu = Lock(clock); val screen = Lock(clock)
        val lease = SharingLease(cpu, screen)
        repeat(12 * 60 * 2) {
            lease.refresh(true)
            clock.now += SharingLease.PERIOD
            assertTrue(cpu.held); assertTrue(screen.held)
        }
        assertTrue(clock.now > 8 * 60 * 60 * 1000)
        lease.close()
        assertFalse(cpu.held); assertFalse(screen.held)
    }
    @Test fun sleepOrOptOutReleasesScreenButRetainsCaptureCpu() {
        val clock = Clock(); val cpu = Lock(clock); val screen = Lock(clock)
        val lease = SharingLease(cpu, screen)
        lease.refresh(true)
        lease.refresh(false)
        assertTrue(cpu.held); assertFalse(screen.held)
        val count = screen.acquisitions
        repeat(10) { clock.now += SharingLease.PERIOD; lease.refresh(false) }
        assertEquals(count, screen.acquisitions)
        lease.close()
        lease.refresh(true)
        assertFalse(cpu.held); assertFalse(screen.held)
    }
    @Test fun abandonedServiceCannotHoldResourcesForever() {
        val clock = Clock(); val cpu = Lock(clock); val screen = Lock(clock)
        val lease = SharingLease(cpu, screen)
        lease.refresh(true)
        clock.now += SharingLease.TIMEOUT + 1
        assertFalse(cpu.held); assertFalse(screen.held)
        lease.refresh(false)
        assertTrue(cpu.held); assertFalse(screen.held)
        lease.close()
    }
}
