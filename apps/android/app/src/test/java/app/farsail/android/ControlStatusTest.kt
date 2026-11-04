package app.farsail.android

import org.junit.Assert.*
import org.junit.Test

class ControlStatusTest {
    private val ready = ControlStatus(true, true, true, true, true, true)
    @Test fun rememberedChoiceCannotMasqueradeAsPermission() {
        for (state in listOf(ready.copy(systemEnabled = false), ready.copy(connected = false), ready.copy(runtimeEnabled = false))) {
            assertFalse(state.active)
            assertFalse(state.directConnection)
            assertFalse(state.detail.contains("已生效"))
        }
        assertTrue(ready.copy(systemEnabled = false).detail.contains("系统辅助功能已关闭"))
        assertTrue(ready.copy(connected = false).detail.contains("等待"))
    }
    @Test fun readyControlStillNeedsCaptureAndAutomaticApproval() {
        assertTrue(ready.active)
        assertTrue(ready.directConnection)
        assertFalse(ready.copy(sharing = false).directConnection)
        assertFalse(ready.copy(automaticApproval = false).directConnection)
        assertTrue(ready.copy(sharing = false).detail.contains("恢复屏幕共享"))
    }
    @Test fun explicitOptOutWinsOverOldRuntimeState() {
        val off = ready.copy(remembered = false)
        assertFalse(off.active)
        assertFalse(off.directConnection)
        assertTrue(off.detail.contains("已关闭"))
    }
}
