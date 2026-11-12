package app.farsail.android

import org.junit.Assert.*
import org.junit.Test

class ConnectionWakePlanTest {
    @Test fun normalHeartbeatsCannotPostponeTheSleepDeadline() {
        val scheduled = mutableListOf<Long>()
        val plan = ConnectionWakePlan({ scheduled.add(it) }, { })
        plan.ensure(0, true); plan.ensure(15000, true); plan.ensure(24000, true)
        assertEquals(listOf(25000L), scheduled)
        assertTrue(plan.delivered())
        plan.ensure(25000, true)
        assertEquals(listOf(25000L, 50000L), scheduled)
    }
    @Test fun foregroundChargingOptOutAndMissingScopeCancelInsteadOfRearming() {
        var cancellations = 0
        val scheduled = mutableListOf<Long>()
        val plan = ConnectionWakePlan({ scheduled.add(it) }, { cancellations++ })
        repeat(4) {
            plan.ensure(it * 100000L, true)
            plan.ensure(it * 100000L + 1, false)
            assertFalse(plan.delivered())
            assertEquals(0L, plan.nextAt)
        }
        assertEquals(4, cancellations)
        assertEquals(4, scheduled.size)
    }
    @Test fun aClosedServiceCannotBeRevivedByLateDelivery() {
        var schedules = 0
        val plan = ConnectionWakePlan({ schedules++ }, { })
        plan.ensure(0, true); plan.close()
        assertFalse(plan.delivered())
        plan.ensure(100000, true)
        assertEquals(1, schedules)
    }
    @Test fun deniedSchedulingDoesNotCreateAFictionalPendingAlarm() {
        var denied = true
        val plan = ConnectionWakePlan({ if (denied) throw SecurityException("not allowed") }, { })
        try { plan.ensure(0, true); fail("schedule should fail") } catch (_: SecurityException) { }
        assertEquals(0L, plan.nextAt)
        assertFalse(plan.delivered())
        denied = false
        plan.ensure(10000, true)
        assertEquals(35000L, plan.nextAt)
    }
}
