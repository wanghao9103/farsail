package app.farsail.android

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class MediaEventLogTest {
    private val first = "12345678-1234-1234-1234-123456789abc"
    private val second = "eeeeeeee-1234-1234-1234-123456789abc"
    private fun session(id: String = first) = JSONObject().put("sessionId", id).put("videoRequested", true)

    @Test fun writesOnlyOnStateTransitionsAndUsesRealSessionCounters() {
        val log = MediaEventLog()
        val m = session().put("published", 18000).put("sent", 17904).put("acknowledged", 17000)
        assertTrue(log.sample(m, 0, 0))
        assertFalse(log.text.contains("本次确认次数"))
        assertFalse(log.text.contains("本次已发帧数"))
        m.put("sentFrames", 3).put("ackedFrames", 1)
        assertTrue(log.sample(m, 100, 100))
        assertTrue(log.text.contains("本次已发帧数 3，本次确认次数 1"))
        assertFalse(log.sample(m.put("sentFrames", 12).put("ackedFrames", 6), 200, 200))
        assertTrue(log.sample(m.put("oldestPendingMs", 2100), 300, 300))
        assertFalse(log.sample(m.put("oldestPendingMs", 2400), 600, 600))
        assertTrue(log.text.contains("画面等待确认达到 2 秒"))
        assertEquals(3, log.text.lines().size)
    }

    @Test fun detectsStalledPublishSequenceOnlyWhileRequested() {
        val log = MediaEventLog()
        val m = session().put("published", 12)
        assertTrue(log.sample(m, 0, 0))
        assertTrue(log.sample(m.put("videoRequested", false), 1, 1))
        assertFalse(log.sample(m, 10000, 10000))
        assertTrue(log.sample(m.put("videoRequested", true), 20000, 20000))
        assertFalse(log.sample(m, 27000, 27000))
        assertTrue(log.sample(m, 28000, 28000))
        assertTrue(log.text.lines().last().contains("发布序号 8 秒未变化"))
        assertTrue(log.sample(m.put("published", 13), 29000, 29000))
        assertTrue(log.text.lines().last().contains("传输中"))
    }

    @Test fun previousCloseIsRecordedOnceAlongsideReplacementAndUntrustedTextIsExcluded() {
        val log = MediaEventLog()
        val closed = session().put("reason", "media_timeout").put("sentFrames", 23).put("ackedFrames", 8)
        val replacement = session(second).put("closedSession", closed).put("rttMs", 1500)
            .put("server", "secret-host.invalid").put("deviceName", "privatePhone")
        assertTrue(log.sample(replacement, 0, 0))
        assertFalse(log.sample(replacement, 100, 100))
        assertEquals(2, log.text.lines().size)
        assertTrue(log.text.lines().first().contains("[12345678] 画面超时  本次已发帧数 23，本次确认次数 8"))
        assertTrue(log.text.lines().last().contains("[eeeeeeee] 首次采样 · 往返延迟达到 1 秒"))
        for (secret in listOf(first, second, "secret-host.invalid", "privatePhone")) assertFalse(log.text.contains(secret))
        val malformed = session("privatePhone").put("closedSession", session(second).put("reason", "secretReason"))
        assertTrue(log.sample(malformed, 200, 200))
        assertFalse(log.text.contains("privatePhone"))
        assertFalse(log.text.contains("secretReason"))
    }

    @Test fun retainedHistoryIsBoundedAndReadableAfterReload() {
        val log = MediaEventLog()
        val m = session()
        repeat(40) { i ->
            assertTrue(log.sample(m.put("videoRequested", i % 2 == 0), i.toLong(), i.toLong()))
        }
        assertEquals(32, log.text.lines().size)
        assertFalse(log.text.contains("首次采样"))
        assertEquals(log.text, MediaEventLog(log.text).text)
    }
}
