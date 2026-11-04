package app.farsail.android

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class ConnectionScopeTest {
    private fun state() = JSONObject().put("signedIn", true).put("server", "https://example.invalid/").put("deviceId", "owned-device")
    @Test fun incompleteOrSignedOutStateCannotMaintainPresence() {
        assertNull(ConnectionService.scope(JSONObject()))
        assertNull(ConnectionService.scope(state().put("signedIn", false)))
        assertNull(ConnectionService.scope(state().put("deviceId", JSONObject.NULL)))
        assertNull(ConnectionService.scope(state().put("server", "")))
    }
    @Test fun presenceIsScopedToAuthenticatedServerAndDevice() {
        assertEquals("https://example.invalid|owned-device", ConnectionService.scope(state()))
        assertNotEquals(ConnectionService.scope(state()), ConnectionService.scope(state().put("deviceId", "another-device")))
        assertNotEquals(ConnectionService.scope(state()), ConnectionService.scope(state().put("server", "https://another.invalid")))
    }
}
