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
    private fun scenarioReport(name: String, fields: JSONObject) {
        fields.put("scenario", name).put("sdk", android.os.Build.VERSION.SDK_INT)
        inst.sendStatus(0, android.os.Bundle().apply { putString("scenarioEvidence", fields.toString()) })
    }
    @Test fun threeMinuteScreenOffAndBusinessBlockKeepHeartbeatsFresh() = withFixture { coordinator ->
        device.pressHome(); await { app.observer == null }; device.sleep()
        await { !context.getSystemService(PowerManager::class.java).isInteractive }
        val entered = java.util.concurrent.CountDownLatch(1)
        val release = java.util.concurrent.CountDownLatch(1)
        val task = app.io.submit { entered.countDown(); release.await() }
        check(entered.await(10, java.util.concurrent.TimeUnit.SECONDS))
        val before = coordinator.successfulHeartbeatTimes.size
        val started = SystemClock.elapsedRealtime()
        var maxAge = 0L
        try {
            repeat(6) {
                Thread.sleep(30000)
                val age = SystemClock.elapsedRealtime() - ConnectionService.lastHeartbeat
                maxAge = maxOf(maxAge, age)
                assertTrue("connection service survives idle", ConnectionService.running)
                assertTrue("blocked business queue cannot consume online TTL", age < 25000)
                scenarioReport("screen-off-business-block-progress", JSONObject().put("elapsedMs", SystemClock.elapsedRealtime()-started).put("heartbeatAgeMs", age))
            }
            assertFalse(context.getSystemService(PowerManager::class.java).isInteractive)
            assertTrue("three minute idle requires continued successful heartbeats", coordinator.successfulHeartbeatTimes.size >= before + 10)
            val times = coordinator.successfulHeartbeatTimes.toList()
            val maxGap = times.zipWithNext { a, b -> b-a }.maxOrNull() ?: 0L
            assertTrue("no successful-heartbeat gap may consume online lease", maxGap < 25000)
            scenarioReport("screen-off-business-block", JSONObject().put("durationMs", SystemClock.elapsedRealtime()-started)
                .put("successesBefore", before).put("successesAfter", times.size).put("maxSampledHeartbeatAgeMs", maxAge).put("maxSuccessfulGapMs", maxGap))
        } finally { release.countDown(); task.get(10, java.util.concurrent.TimeUnit.SECONDS) }
    }
    @Test fun repeatedSocketDisconnectsRecoverWithoutRestartingService() = withFixture { coordinator ->
        val pid = android.os.Process.myPid()
        repeat(3) { cycle ->
            val previous = ConnectionService.lastHeartbeat
            val attempted = coordinator.heartbeats.get()
            coordinator.dropHeartbeatResponses.set(true)
            try {
                await { coordinator.heartbeats.get() > attempted && !ConnectionService.detail.startsWith("设备在线") }
                assertTrue(ConnectionService.running)
                assertEquals("disconnected response is not a successful heartbeat", previous, ConnectionService.lastHeartbeat)
            } finally { coordinator.dropHeartbeatResponses.set(false) }
            await { ConnectionService.lastHeartbeat > previous && ConnectionService.detail.startsWith("设备在线") }
            assertEquals(pid, android.os.Process.myPid())
            scenarioReport("socket-disconnect-recovery", JSONObject().put("cycle", cycle+1).put("sameProcess", true).put("running", ConnectionService.running))
        }
    }
    @Test fun actualSlowAccountHttpDoesNotBlockStandbyHeartbeats() = withFixture { coordinator ->
        await { SystemClock.elapsedRealtime()-ConnectionService.lastHeartbeat >= 5000 }
        coordinator.slowMe.set(true)
        val business = app.io.submit { app.invoke("resume") }
        try {
            assertTrue(coordinator.meSeen.await(10, java.util.concurrent.TimeUnit.SECONDS))
            val previous = ConnectionService.lastHeartbeat
            await { ConnectionService.lastHeartbeat > previous }
            assertFalse("account HTTP must still be awaiting its reply", business.isDone)
            assertTrue(ConnectionService.running)
            scenarioReport("slow-account-http", JSONObject().put("heartbeatArrivedBeforeBusinessReply", true).put("heartbeatAgeMs", SystemClock.elapsedRealtime()-ConnectionService.lastHeartbeat))
        } finally {
            coordinator.slowMe.set(false); coordinator.releaseMe.countDown()
            business.get(20, java.util.concurrent.TimeUnit.SECONDS)
        }
    }
    @Test fun lateHeartbeatResponseAfterOptOutCannotRestartStandby() = withFixture { coordinator ->
        coordinator.holdHeartbeatResponses.set(true)
        try {
            assertTrue(coordinator.heartbeatHeld.await(25, java.util.concurrent.TimeUnit.SECONDS))
            inst.runOnMainSync { ConnectionService.preference(context, false) }
            await { !ConnectionService.running }
            val previous = ConnectionService.lastHeartbeat
            coordinator.holdHeartbeatResponses.set(false); coordinator.releaseHeartbeats.countDown()
            Thread.sleep(2000)
            val requests = coordinator.heartbeats.get()
            Thread.sleep(17000)
            assertFalse(ConnectionService.running)
            assertEquals("late response cannot publish success after opt out", previous, ConnectionService.lastHeartbeat)
            assertEquals("opt out prevents subsequent heartbeat requests", requests, coordinator.heartbeats.get())
            scenarioReport("late-response-after-opt-out", JSONObject().put("running", false).put("newRequestsAfterStop", 0).put("lateSuccessIgnored", true))
        } finally { coordinator.holdHeartbeatResponses.set(false); coordinator.releaseHeartbeats.countDown() }
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
