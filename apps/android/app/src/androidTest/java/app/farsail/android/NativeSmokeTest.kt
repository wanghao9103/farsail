package app.farsail.android

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.net.ServerSocket
import java.util.concurrent.atomic.AtomicBoolean

@RunWith(AndroidJUnit4::class)
class NativeSmokeTest {
    private fun await(condition: () -> Boolean) {
        val end = System.currentTimeMillis() + 20000
        while (System.currentTimeMillis() < end) { if (condition()) return; Thread.sleep(100) }
        assertTrue("condition timed out", condition())
    }
    @Test fun rejectedSharingKeepsFailureVisibleAfterServiceStops() {
        org.junit.Assume.assumeTrue("Dedicated emulator only", android.os.Build.MODEL.contains("sdk", ignoreCase = true))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        val inst = InstrumentationRegistry.getInstrumentation(); val context = inst.targetContext
        val app = context.applicationContext as FarSailApp; await { app.ready }
        runCatching { app.invoke("logout") }
        val coordinator = FakeCoordinator(true)
        try {
            app.invoke("server", JSONObject().put("url", "http://127.0.0.1:${coordinator.port}"))
            app.invoke("login", JSONObject().put("email", "android-smoke@example.invalid").put("password", "synthetic-password"))
            app.invoke("bind", JSONObject().put("name", "Android failure fixture")); app.refresh()
            context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            val device = UiDevice.getInstance(inst)
            assertTrue(device.wait(Until.hasObject(By.text("开启屏幕共享")), 10000)); device.findObject(By.text("开启屏幕共享")).click()
            val allow = device.wait(Until.findObject(By.res("android", "button1")), 10000); assertNotNull(allow); allow.click()
            await { CaptureService.phase == "failed" && !CaptureService.running }
            device.takeScreenshot(java.io.File(context.filesDir, "sharing-failed-before-check.png"))
            assertTrue(device.wait(Until.hasObject(By.text("开启共享失败")), 5000))
            assertTrue(device.wait(Until.hasObject(By.textContains("服务器拒绝安卓共享（409）")), 5000))
            assertNotNull(device.findObject(By.text("重新开启共享")))
            assertFalse((app.invoke("state") as JSONObject).optBoolean("sharing"))
            device.takeScreenshot(java.io.File(context.filesDir, "sharing-failed-409.png"))
        } finally { CaptureService.stop(context); runCatching { app.invoke("logout") }; coordinator.close() }
    }
    @Test fun keystoreJniProjectionBackgroundAndStop() {
        org.junit.Assume.assumeTrue("Dedicated emulator only", android.os.Build.FINGERPRINT.contains("generic") || android.os.Build.MODEL.contains("sdk", ignoreCase = true))
        Configurator.getInstance().uiAutomationFlags = android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES
        val inst = InstrumentationRegistry.getInstrumentation()
        val context = inst.targetContext
        val app = context.applicationContext as FarSailApp
        await { app.ready }
        val store = CredentialStore(context)
        val bytes = "synthetic-secret-for-test".toByteArray()
        store.write("smoke-secret", bytes)
        assertArrayEquals(bytes, store.read("smoke-secret"))
        val disk = java.io.File(context.noBackupFilesDir, "credentials/smoke-secret.bin")
        assertFalse(disk.readBytes().toString(Charsets.ISO_8859_1).contains("synthetic-secret"))
        val encrypted = disk.readBytes(); encrypted[encrypted.lastIndex] = (encrypted.last().toInt() xor 1).toByte(); disk.writeBytes(encrypted)
        try { store.read("smoke-secret"); fail("tamper must fail") } catch (_: javax.crypto.AEADBadTagException) { }
        store.delete("smoke-secret")
        assertNull(store.read("smoke-secret"))
        val readyFile = java.io.File(context.filesDir, "p2p-ready.json").apply { delete() }
        val realPeer = InstrumentationRegistry.getArguments().getString("realPeer") == "true"
        val coordinator = FakeCoordinator()
        try {
            app.invoke("server", JSONObject().put("url", "http://127.0.0.1:${coordinator.port}"))
            app.invoke("login", JSONObject().put("email", "android-smoke@example.invalid").put("password", "synthetic-password"))
            app.invoke("bind", JSONObject().put("name", "Android emulator smoke"))
            assertTrue(coordinator.androidBound.get())
            app.refresh()
            context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            val device = UiDevice.getInstance(inst)
            assertTrue(device.wait(Until.hasObject(By.text("开启屏幕共享")), 10000))
            assertTrue(device.wait(Until.hasObject(By.text("○ 屏幕共享未开启")), 5000))
            device.findObject(By.text("开启屏幕共享")).click()
            // The system dialog is interacted with visibly; no projection token is fabricated.
            val allow = device.wait(Until.findObject(By.res("android", "button1")), 10000)
            assertNotNull("system projection consent", allow)
            allow.click()
            await { CaptureService.running && (app.invoke("state") as JSONObject).optBoolean("sharing") }
            assertTrue(coordinator.hosting.get())
            await { CaptureService.phase == "sharing" }
            assertTrue(device.wait(Until.hasObject(By.text("● 屏幕共享中")), 5000))
            device.pressHome()
            Thread.sleep(2500)
            assertTrue("foreground projection must survive leaving Activity", CaptureService.running)
            assertTrue((app.invoke("state") as JSONObject).optBoolean("sharing"))
            assertFalse(NativeBridge.inputValid("unapproved-session", 0, 1))
            if (realPeer) {
                val automation = inst.getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
                automation.executeShellCommand("settings put secure enabled_accessibility_services app.farsail.android/app.farsail.android.ControlService").close()
                automation.executeShellCommand("settings put secure accessibility_enabled 1").close()
                await { ControlService.instance != null }
                NativeBridge.setControl(true)
                coordinator.requestReady.set(true)
                app.invoke("pending")
                await { coordinator.approved.get() }
                assertFalse("foreign account must not be auto-approved", coordinator.foreignApproved.get())
                context.startActivity(Intent(inst.context, InputFixtureActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                assertTrue(device.wait(Until.hasObject(By.desc("remote-input-fixture")), 10000))
                await { coordinator.endpoint.get() != null }
                val target = device.findObject(By.desc("remote-click-target")).visibleBounds
                val endpoint = JSONObject(coordinator.endpoint.get().toString())
                    .put("tapX", target.centerX().toDouble() / CaptureService.screenWidth)
                    .put("tapY", target.centerY().toDouble() / CaptureService.screenHeight)
                readyFile.writeText(endpoint.toString())
                assertTrue("real QUIC desktop text must reach owned Android input", device.wait(Until.hasObject(By.text("desktop-android-smoke")), 45000))
                assertTrue("real mouse click must reach owned Android button", device.wait(Until.hasObject(By.text("远程点击成功")), 15000))
                Thread.sleep(4500)
                assertTrue(CaptureService.running)
            }
            // Removing the Activity task must leave the explicit projection service active.
            context.getSystemService(android.app.ActivityManager::class.java).appTasks.forEach { it.finishAndRemoveTask() }
            Thread.sleep(1000)
            assertTrue("projection survives Activity task removal", CaptureService.running)
            inst.runOnMainSync { app.observer = null }
            context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            device.wait(Until.hasObject(By.text("停止共享")), 5000)
            device.findObject(By.text("停止共享")).click()
            await { !CaptureService.running && !(app.invoke("state") as JSONObject).optBoolean("sharing") }
            assertFalse(NativeBridge.inputValid("unapproved-session", 0, 1))
            await { !coordinator.hosting.get() }
            val nm = context.getSystemService(android.app.NotificationManager::class.java)
            await { nm.activeNotifications.none { it.id == 41 } }
        } finally {
            NativeBridge.stopLocal(); CaptureService.stop(context)
            try { app.invoke("logout") } catch (_: Exception) { }
            coordinator.close()
        }
    }
}

/** Synthetic coordination responses only. The transport/media host is the real JNI library. */
private class FakeCoordinator(private val rejectSharing: Boolean = false) : java.io.Closeable {
    private val socket = ServerSocket(0, 20, java.net.InetAddress.getByName("127.0.0.1"))
    val port get() = socket.localPort
    val androidBound = AtomicBoolean(false)
    val hosting = AtomicBoolean(false)
    val requestReady = AtomicBoolean(false)
    val approved = AtomicBoolean(false)
    val foreignApproved = AtomicBoolean(false)
    val endpoint = java.util.concurrent.atomic.AtomicReference<JSONObject?>()
    private val source = InstrumentationRegistry.getArguments().getString("sourceKey", "")
    private val thread = Thread {
        while (!socket.isClosed) {
            val client = try { socket.accept() } catch (_: Exception) { break }
            client.use {
                try {
                    val input = it.getInputStream()
                    fun line(): String {
                        val bytes = java.io.ByteArrayOutputStream()
                        while (true) { val b = input.read(); if (b == -1 || b == 10) break; if (b != 13) bytes.write(b) }
                        return bytes.toString("UTF-8")
                    }
                    val request = line(); var length = 0
                    while (true) { val h = line(); if (h.isEmpty()) break; if (h.startsWith("Content-Length:", true)) length = h.substringAfter(':').trim().toInt() }
                    val bytes = ByteArray(length); java.io.DataInputStream(input).readFully(bytes)
                    val body = if (length > 0) JSONObject(String(bytes, Charsets.UTF_8)) else JSONObject()
                    val path = request.split(' ').getOrElse(1) { "/" }
                    var status = 200
                    val response = when (path.substringBefore('?')) {
                        "/v1/auth/login", "/v1/auth/refresh" -> """{"access_token":"synthetic-access","refresh_token":"synthetic-refresh","session_id":"10000000-0000-4000-8000-000000000001","access_expires_in":900}"""
                        "/v1/me" -> """{"id":"20000000-0000-4000-8000-000000000001","email":"android-smoke@example.invalid","email_verified":true,"role":"user"}"""
                        "/v1/devices/challenge" -> """{"challenge_id":"30000000-0000-4000-8000-000000000001","message":"synthetic-binding-challenge"}"""
                        "/v1/devices/bind" -> { androidBound.set(body.optString("platform") == "android" && body.optString("signature").length == 128); """{"id":"40000000-0000-4000-8000-000000000001","device_token":"synthetic-device"}""" }
                        "/v1/devices/heartbeat" -> """{"generation":1,"expires_in":60}"""
                        "/v1/devices/capability" -> if (rejectSharing && body.optBoolean("can_host")) { status = 409; """{"error":"conflict"}""" } else { hosting.set(body.optBoolean("can_host")); "{}" }
                        "/v1/devices/endpoint-address" -> { endpoint.set(body.getJSONObject("endpoint_addr")); "{}" }
                        "/v1/remote/pending" -> if (requestReady.get()) """[{"id":"50000000-0000-4000-8000-000000000002","permission":"control","requester_id":"20000000-0000-4000-8000-000000000002","requester_email":"foreign@example.invalid"},{"id":"50000000-0000-4000-8000-000000000001","permission":"control","requester_id":"20000000-0000-4000-8000-000000000001","requester_email":"smoke@example.invalid","source_device_name":"Synthetic desktop"}]""" else "[]"
                        "/v1/remote/50000000-0000-4000-8000-000000000002/decide" -> { foreignApproved.set(true); "{}" }
                        "/v1/remote/sessions" -> "[]"
                        "/v1/remote/50000000-0000-4000-8000-000000000001/decide" -> { approved.set(true); requestReady.set(false); """{"permission":"control","grant_token":"synthetic-grant"}""" }
                        "/v1/remote/50000000-0000-4000-8000-000000000001/transport-grant" -> JSONObject().put("session_id", "50000000-0000-4000-8000-000000000001").put("permission", "control").put("source_public_key", source).put("target_public_key", endpoint.get()!!.getString("id")).put("nonce", "ab".repeat(32)).put("expires_in", 25).toString()
                        "/v1/remote/50000000-0000-4000-8000-000000000001/transport-renew" -> """{"permission":"control","grant_token":"synthetic-grant"}"""
                        else -> "{}"
                    }.toByteArray()
                    it.getOutputStream().write("HTTP/1.1 $status Result\r\nContent-Type: application/json\r\nContent-Length: ${response.size}\r\nConnection: close\r\n\r\n".toByteArray() + response)
                } catch (_: Exception) { }
            }
        }
    }.apply { isDaemon = true; start() }
    override fun close() { socket.close(); thread.join(2000) }
}
