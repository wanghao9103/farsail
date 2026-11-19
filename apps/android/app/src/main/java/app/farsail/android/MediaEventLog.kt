package app.farsail.android

import org.json.JSONObject
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/** Translates sampled, session-scoped media counters into a bounded local event history. */
internal class MediaEventLog(saved: String = "") {
    companion object {
        private const val LIMIT = 32
        private val UUID = Regex("[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}")
        private val TIME = DateTimeFormatter.ofPattern("MM-dd HH:mm:ss")
    }

    private val records = ArrayDeque(saved.lineSequence().filter { it.isNotBlank() }.toList().takeLast(LIMIT))
    private var currentId: String? = null
    private var closedId: String? = null
    private var lastState = ""
    private var wasRequested = false
    private var hadAck = false
    private var lastPublished = -1L
    private var publishedSince = 0L
    val text get() = records.joinToString("\n")

    fun sample(media: JSONObject, now: Long, elapsed: Long): Boolean {
        var changed = false
        val closed = media.optJSONObject("closedSession")
        val endedId = closed?.optString("sessionId")
        if (closed != null && endedId != null && UUID.matches(endedId) && endedId != closedId) {
            val reason = when (closed.optString("reason")) {
                "media_timeout" -> "画面超时"
                "lease_expired" -> "授权到期"
                "network_timeout" -> "网络超时"
                "stopped" -> "已停止"
                "authorization_failed" -> "授权失败"
                "renewal_failed" -> "续期失败"
                "capture_failed" -> "采集失败"
                "input_failed" -> "输入失败"
                "input_sequence" -> "输入顺序异常"
                else -> "连接结束"
            }
            add(now, endedId, reason, closed)
            closedId = endedId
            changed = true
        }

        val id = media.optString("sessionId")
        if (!UUID.matches(id)) {
            currentId = null
            return changed
        }
        val requested = media.optBoolean("videoRequested", true)
        val rtt = media.optLong("rttMs", -1)
        val pendingAge = media.optLong("oldestPendingMs", -1)
        val published = media.optLong("published", -1)
        if (id != currentId || published != lastPublished || !requested || !wasRequested)
            publishedSince = elapsed
        lastPublished = published
        val state = when {
            !requested -> "画面已暂停"
            pendingAge >= 8_000 -> "画面等待确认达到 8 秒"
            pendingAge >= 2_000 -> "画面等待确认达到 2 秒"
            rtt >= 1_000 -> "往返延迟达到 1 秒"
            rtt >= 250 -> "往返延迟达到 250 毫秒"
            published >= 0 && elapsed - publishedSince >= 8_000 -> "发布序号 8 秒未变化"
            else -> "传输中"
        }
        val ack = media.optLong("ackedFrames", 0) > 0
        val event = when {
            id != currentId -> "首次采样 · $state"
            state != lastState -> state
            ack && !hadAck -> "首次收到画面确认"
            else -> null
        }
        currentId = id
        lastState = state
        wasRequested = requested
        hadAck = ack
        if (event != null) {
            add(now, id, event, media)
            changed = true
        }
        return changed
    }

    private fun add(now: Long, id: String, event: String, sample: JSONObject) {
        val metrics = mutableListOf<String>()
        fun metric(key: String, unit: String, label: String) {
            val n = sample.optLong(key, -1)
            if (n >= 0) metrics += "$label $n$unit"
        }
        metric("rttMs", "ms", "RTT")
        metric("oldestPendingMs", "ms", "待确认")
        if (sample.optLong("sentFrames", 0) > 0) metric("sendMs", "ms", "发送耗时")
        if (sample.optLong("encodedWidth", 0) > 0) metric("encodeMs", "ms", "编码耗时")
        metric("published", "", "最新发布序号")
        metric("sentFrames", "", "本次已发帧数")
        metric("ackedFrames", "", "本次确认次数")
        val stamp = TIME.format(Instant.ofEpochMilli(now).atZone(ZoneId.systemDefault()))
        if (records.size == LIMIT) records.removeFirst()
        records.addLast("$stamp [${id.take(8)}] $event  ${metrics.joinToString("，")}")
    }
}
