package app.farsail.android

import android.content.Intent
import android.os.PowerManager
import android.os.SystemClock
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Never run power, account or permission fixtures against a personal phone. */
@RunWith(AndroidJUnit4::class)
class ConnectionServiceTest {
    private val inst get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = inst.targetContext
    private val app get() = context.applicationContext as FarSailApp
    private val device get() = UiDevice.getInstance(inst)
    private fun await(test: () -> Boolean) {
        val end = SystemClock.elapsedRealtime() + 25000
        while (SystemClock.elapsedRealtime() < end) { if (test()) return; Thread.sleep(100) }
        assertTrue("connection condition timed out", test())
    }
    private fun shell(cmd: String) = android.os.ParcelFileDescriptor.AutoCloseInputStream(inst.getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES).executeShellCommand(cmd)).bufferedReader().use { it.readText().trim() }
    private fun wakeLocks() = shell("dumpsys power").substringAfter("Wake Locks:").substringBefore("Suspend Blockers:")
    private fun withFixture(block: (FakeCoordinator) -> Unit) {
        org.junit.Assume.assumeTrue("Owned emulator only", shell("getprop ro.kernel.qemu") == "1" && android.os.Build.MODEL.contains("sdk", true))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        device.wakeUp(); shell("wm dismiss-keyguard"); device.pressMenu()
        await { context.getSystemService(PowerManager::class.java).isInteractive && !context.getSystemService(android.app.KeyguardManager::class.java).isKeyguardLocked && app.ready }
        val old = ConnectionService.enabled(context)
        val coordinator = FakeCoordinator()
        try {
            inst.runOnMainSync { ConnectionService.preference(context, false) }
            await { !ConnectionService.running }
            CaptureService.stop(context); await { !CaptureService.running }
            runCatching { app.invoke("logout") }; RemotePreferences.clear(context)
            app.invoke("server", JSONObject().put("url", "http://127.0.0.1:${coordinator.port}"))
            app.invoke("login", JSONObject().put("email", "connection@example.invalid").put("password", "synthetic-password"))
            app.invoke("bind", JSONObject().put("name", "Owned connection fixture"))
            app.io.submit { app.refresh() }.get(15, java.util.concurrent.TimeUnit.SECONDS)
            await { app.state.optBoolean("signedIn") && app.state.optString("server") == "http://127.0.0.1:${coordinator.port}" }
            inst.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            inst.waitForIdleSync()
            if (!device.wait(Until.hasObject(By.text("设置")), 10000)) {
                device.dumpWindowHierarchy(java.io.File(context.filesDir, "connection-home-failure.xml"))
                device.takeScreenshot(java.io.File(context.filesDir, "connection-home-failure.png"))
                fail("Connection fixture did not reach the authenticated Home page")
            }
            coordinator.hosting.set(true) // Stale cloud advertisement from a stopped recording.
            inst.runOnMainSync { ConnectionService.preference(context, true) }
            await { ConnectionService.running && ConnectionService.lastHeartbeat > 0 && coordinator.heartbeats.get() > 0 && !coordinator.hosting.get() }
            block(coordinator)
        } finally {
            inst.runOnMainSync { ConnectionService.preference(context, false) }
            CaptureService.stop(context); await { !ConnectionService.running && !CaptureService.running }
            runCatching { app.invoke("logout") }; RemotePreferences.clear(context)
            app.io.submit { app.refresh() }.get(15, java.util.concurrent.TimeUnit.SECONDS)
            await { !app.state.optBoolean("signedIn") }
            inst.runOnMainSync { ConnectionService.preference(context, old) }
            coordinator.close(); device.wakeUp(); shell("wm dismiss-keyguard"); device.pressHome()
        }
    }
    @Test fun blockedBusinessQueueCannotDelayHeartbeatPastOnlineLease() = withFixture { coordinator ->
        val entered = java.util.concurrent.CountDownLatch(1)
        val release = java.util.concurrent.CountDownLatch(1)
        val task = app.io.submit { entered.countDown(); release.await() }
        check(entered.await(10, java.util.concurrent.TimeUnit.SECONDS))
        val before = coordinator.heartbeats.get()
        try {
            // Keep screen on and use loopback HTTP to isolate business starvation.
            Thread.sleep(35000); Thread.sleep(30000)
            assertTrue(ConnectionService.running)
            assertTrue(context.getSystemService(PowerManager::class.java).isInteractive)
            assertTrue("heartbeats must bypass the blocked application queue", coordinator.heartbeats.get() >= before + 4)
            val age = SystemClock.elapsedRealtime() - ConnectionService.lastHeartbeat
            assertTrue("online lease must remain fresh", age < 25000)
            java.io.File(context.filesDir, "heartbeat-queue-fixed.json").writeText(JSONObject()
                .put("heartbeatsBefore", before).put("heartbeatsDuringBlock", coordinator.heartbeats.get())
                .put("heartbeatAgeMs", age).put("businessBlockedMs", 65000).toString())
        } finally { release.countDown(); task.get(10, java.util.concurrent.TimeUnit.SECONDS) }
    }
    @Test fun temporaryServerFailureKeepsStandbyAndRecovers() = withFixture { coordinator ->
        val last = ConnectionService.lastHeartbeat
        val count = coordinator.heartbeats.get()
        coordinator.heartbeatFailure.set(503)
        try {
            await { coordinator.heartbeats.get() > count && !ConnectionService.detail.startsWith("设备在线") }
            assertTrue("transient server failure must not revoke local standby", ConnectionService.running)
            assertEquals("failed request cannot advance heartbeat time", last, ConnectionService.lastHeartbeat)
        } finally { coordinator.heartbeatFailure.set(0) }
        await { ConnectionService.lastHeartbeat > last && ConnectionService.detail.startsWith("设备在线") }
    }
    @Test fun logoutWhileBusinessQueueIsBlockedStopsIndependentHeartbeats() = withFixture { coordinator ->
        val entered = java.util.concurrent.CountDownLatch(1)
        val release = java.util.concurrent.CountDownLatch(1)
        val task = app.io.submit { entered.countDown(); release.await() }
        check(entered.await(10, java.util.concurrent.TimeUnit.SECONDS))
        try {
            app.invoke("logout")
            await { !ConnectionService.running }
            val stopped = coordinator.heartbeats.get()
            Thread.sleep(17000)
            assertEquals("old independent heartbeat must not survive logout", stopped, coordinator.heartbeats.get())
        } finally { release.countDown(); task.get(10, java.util.concurrent.TimeUnit.SECONDS) }
        app.io.submit { }.get(15, java.util.concurrent.TimeUnit.SECONDS)
        assertFalse(ConnectionService.running)
    }
    @Test fun screenOffKeepsHeartbeatsBeyondOnlineTtlWithoutCapture() = withFixture { coordinator ->
        val state = app.invoke("state") as JSONObject
        assertFalse(CaptureService.running); assertFalse(state.optBoolean("sharing")); assertFalse(state.optBoolean("remoteWatch"))
        val pid = android.os.Process.myPid(); val before = coordinator.heartbeats.get()
        device.pressHome(); await { app.observer == null }; device.sleep()
        await { !context.getSystemService(PowerManager::class.java).isInteractive }
        Thread.sleep(70000) // Longer than the coordinator's 60-second online lease.
        assertTrue("standby outlives recording", ConnectionService.running)
        assertTrue("heartbeats continue while the physical display is off", coordinator.heartbeats.get() >= before + 4)
        assertTrue(SystemClock.elapsedRealtime() - ConnectionService.lastHeartbeat < 25000)
        assertEquals(pid, android.os.Process.myPid())
        assertFalse(CaptureService.running)
        assertFalse((app.invoke("state") as JSONObject).optBoolean("sharing"))
        assertTrue(wakeLocks().contains("FarSail:Connection"))
        assertFalse(wakeLocks().contains("FarSail:SharingScreen"))
        inst.runOnMainSync { ConnectionService.preference(context, false) }
        await { !ConnectionService.running }
        app.io.submit { }.get(15, java.util.concurrent.TimeUnit.SECONDS)
        val stopped = coordinator.heartbeats.get(); Thread.sleep(17000)
        assertEquals("opting out stops future heartbeats", stopped, coordinator.heartbeats.get())
        assertFalse(wakeLocks().contains("FarSail:Connection"))
        device.wakeUp(); shell("wm dismiss-keyguard")
        context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        Thread.sleep(500); assertFalse("foreground must respect opt out", ConnectionService.running)
    }
    @Test fun stoppingRecordingKeepsPresenceButLogoutStopsIt() = withFixture { coordinator ->
        openSharingSettings(device)
        val start = device.wait(Until.findObject(By.text("开启屏幕共享")), 10000)
        assertNotNull(start); start.click()
        val grant = device.wait(Until.findObject(By.res("android", "button1")), 10000)
        assertNotNull(grant); grant.click(); await { CaptureService.phase == "sharing" }
        CaptureService.stop(context); await { !CaptureService.running && !coordinator.hosting.get() }
        val before = coordinator.heartbeats.get()
        await { coordinator.heartbeats.get() > before }
        assertTrue(ConnectionService.running)
        assertFalse((app.invoke("state") as JSONObject).optBoolean("sharing"))
        app.invoke("logout")
        await { !ConnectionService.running }
        assertFalse(wakeLocks().contains("FarSail:Connection"))
    }
    @Test fun savedScopeRestoresServiceButCannotFollowAnotherIdentity() = withFixture { coordinator ->
        val oldBeat = ConnectionService.lastHeartbeat
        context.stopService(Intent(context, ConnectionService::class.java))
        await { !ConnectionService.running }
        context.startForegroundService(Intent(context, ConnectionService::class.java))
        await { ConnectionService.running && ConnectionService.lastHeartbeat > oldBeat }
        assertFalse(CaptureService.running)
        context.stopService(Intent(context, ConnectionService::class.java))
        await { !ConnectionService.running }
        app.io.submit { }.get(15, java.util.concurrent.TimeUnit.SECONDS)
        val before = coordinator.heartbeats.get()
        context.getSharedPreferences("connection", android.content.Context.MODE_PRIVATE).edit().putString("scope", "https://another.invalid|another-device").commit()
        context.startForegroundService(Intent(context, ConnectionService::class.java))
        await { ConnectionService.lastHeartbeat == 0L && !ConnectionService.running }
        assertEquals("a stale identity cannot send presence for the new account", before, coordinator.heartbeats.get())
        assertFalse((app.invoke("state") as JSONObject).optBoolean("sharing"))
    }
    @Test fun revokedDeviceAuthorizationStopsStandby() = withFixture { coordinator ->
        coordinator.rejectHeartbeats.set(true)
        await { !ConnectionService.running }
        app.io.submit { }.get(15, java.util.concurrent.TimeUnit.SECONDS)
        val stopped = coordinator.heartbeats.get(); Thread.sleep(17000)
        assertEquals("revoked authorization must not trigger an endless retry loop", stopped, coordinator.heartbeats.get())
        assertFalse(wakeLocks().contains("FarSail:Connection"))
        assertFalse(ConnectionService.detail.startsWith("设备在线"))
    }
}
