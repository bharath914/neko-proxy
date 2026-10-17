package dev.proxy.inspector.internal

import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean

/** What an HTTP stack module knows when a call starts. */
class CallInfo(
    val method: String,
    val url: String,
    val headers: List<Pair<String, String>>,
    val clientStack: String,
    val routeTemplate: String = "",
    val name: String = "",
    val owner: String = "",
    val tag: String = "",
    val requestBodySize: Long = 0,
    val httpVersion: String = "",
)

/** The entry point for OkHttp and Ktor modules. */
object Recorder {
    /** One volatile read. The whole cost of a request while no desktop is connected. */
    @JvmStatic
    fun active(): Boolean = Runtime.live != null

    /** Starts recording a call, or returns null when nothing should be captured for it. */
    fun begin(info: CallInfo): CallHandle? {
        val live = Runtime.live ?: return null
        val host = hostOf(info.url)
        if (!live.hostAllowed(host)) return null
        return CallHandle(live, info)
    }

    private fun hostOf(url: String): String {
        val s = url.indexOf("://").let { if (it < 0) 0 else it + 3 }
        var e = s
        while (e < url.length && url[e] !in "/?#") e++
        val authority = url.substring(s, e).substringAfterLast('@')
        return if (authority.startsWith("[")) authority.substringBefore(']').trim('[') else authority.substringBefore(':')
    }
}

/** A bounded copy of a body as the app reads or writes it. */
class Tee(private val cap: Int) {
    private var buf = ByteArray(0)
    var length = 0
        private set
    var total = 0L
        private set
    var truncated = false
        private set

    fun write(b: ByteArray, off: Int, len: Int) {
        if (len <= 0) return
        total += len
        val room = cap - length
        val take = if (len < room) len else room
        if (take > 0) {
            if (length + take > buf.size) buf = buf.copyOf(maxOf(minOf(cap, maxOf(buf.size * 2, 1024)), length + take))
            System.arraycopy(b, off, buf, length, take)
            length += take
        }
        if (take < len) truncated = true
    }

    fun bytes(): ByteArray = if (buf.size == length) buf else buf.copyOf(length)

    /** Bytes captured since [from], for live streaming chunks. */
    fun slice(from: Int): ByteArray = buf.copyOfRange(from, length)
}

/**
 * One call in flight. Methods run on the app's thread and only snapshot and copy; serialization,
 * redaction and compression happen later on the sender thread.
 */
class CallHandle(private val live: Live, info: CallInfo) {
    val callId: String = UUID.randomUUID().toString()
    private val engine = live.engine
    private val cfg = live.config
    private val startedAt = System.currentTimeMillis()
    private val finished = AtomicBoolean(false)

    private val bodiesOn = live.redactor.bodiesAllowed
    private var reqTee: Tee? = null
    private var resTee: Tee? = null
    private var reqContentType = ""
    private var resContentType = ""
    private var reqSkipped = !bodiesOn
    private var resSkipped = !bodiesOn
    private var resSize = 0L
    private var streaming = false
    private var streamed = 0
    private var streamSeq = 0
    private var lastStream = 0L

    init {
        reqContentType = info.headers.firstOrNull { it.first.equals("content-type", true) }?.second.orEmpty()
        reqSkipped = reqSkipped || skipType(reqContentType, info.requestBodySize)
        if (!reqSkipped) reqTee = Tee(cfg.bodyCapBytes)
        val screen = engine.screen
        val r = live.redactor.fork()
        engine.emit { sink ->
            sink.send(Envelope(Envelope.CALL_START, CallStart(
                callId = callId, startedAtMs = startedAt, method = info.method, url = r.url(info.url),
                headers = r.headers(info.headers), httpVersion = info.httpVersion, clientStack = info.clientStack,
                routeTemplate = info.routeTemplate, name = info.name, owner = info.owner, tag = info.tag,
                screen = screen, requestBodySize = info.requestBodySize,
            ).encode()))
        }
    }

    fun onRequestBody(b: ByteArray, off: Int, len: Int) { reqTee?.write(b, off, len) }

    fun responseStarted(status: Int, headers: List<Pair<String, String>>, httpVersion: String, size: Long) {
        resContentType = headers.firstOrNull { it.first.equals("content-type", true) }?.second.orEmpty()
        resSize = size
        streaming = resContentType.startsWith("text/event-stream", true)
        resSkipped = resSkipped || skipType(resContentType, size)
        if (!resSkipped) resTee = Tee(cfg.bodyCapBytes)
        val r = live.redactor.fork()
        engine.emit { sink ->
            sink.send(Envelope(Envelope.RESPONSE_START, ResponseStart(callId, status, r.headers(headers), httpVersion, size).encode()))
        }
    }

    fun onResponseBody(b: ByteArray, off: Int, len: Int) {
        val t = resTee ?: run { resSize += 0; return }
        t.write(b, off, len)
        if (streaming) flushStream(t, false)
    }

    /** Server-sent events and other streams: send what has arrived, every so often. */
    private fun flushStream(t: Tee, final: Boolean) {
        val now = System.currentTimeMillis()
        val pending = t.length - streamed
        if (pending <= 0 && !final) return
        if (!final && pending < 4096 && now - lastStream < 250) return
        lastStream = now
        val piece = if (pending > 0) t.slice(streamed) else ByteArray(0)
        streamed = t.length
        val seq = streamSeq++
        val truncated = t.truncated && final
        engine.emit { sink ->
            Bodies.sendChunks(sink, callId, Part.RESPONSE, piece, seq, last = final, truncated = truncated)
        }
    }

    fun complete(timings: Timings?, fromCache: Boolean) {
        if (!finished.compareAndSet(false, true)) return
        val ended = System.currentTimeMillis()
        val req = reqTee; val res = resTee
        val r = live.redactor.fork()
        val eager = cfg.eagerBodyBytes
        if (streaming && res != null) flushStream(res, true)
        val reqBytes = req?.bytes(); val reqTrunc = req?.truncated == true
        val resBytes = if (streaming) null else res?.bytes(); val resTrunc = res?.truncated == true
        val reqTotal = req?.total ?: 0L
        val resTotal = res?.total ?: resSize
        val reqCt = reqContentType; val resCt = resContentType
        val reqSk = reqSkipped; val resSk = resSkipped
        val streamed = streaming
        engine.emit { sink ->
            var reqAvail = true; var resAvail = true
            if (reqBytes != null && reqBytes.isNotEmpty()) {
                val body = r.body(reqCt, reqBytes)
                if (body.size <= eager) Bodies.sendChunks(sink, callId, Part.REQUEST, body, 0, last = true, truncated = reqTrunc)
                else { engine.ring.put(callId, Part.REQUEST, body); reqAvail = false }
            }
            if (resBytes != null && resBytes.isNotEmpty()) {
                val body = r.body(resCt, resBytes)
                if (body.size <= eager) Bodies.sendChunks(sink, callId, Part.RESPONSE, body, 0, last = true, truncated = resTrunc)
                else { engine.ring.put(callId, Part.RESPONSE, body); resAvail = false }
            }
            sink.send(Envelope(Envelope.CALL_COMPLETE, CallComplete(
                callId = callId, endedAtMs = ended, timings = timings, fromCache = fromCache,
                responseBodyAvailable = resAvail || streamed, requestBodyAvailable = reqAvail,
                requestRedacted = r.changed, responseRedacted = r.changed,
                responseBodySkipped = resSk, requestBodySkipped = reqSk,
                responseBodySize = resTotal, requestBodySize = reqTotal,
            ).encode()))
        }
    }

    fun fail(kind: String, message: String, timings: Timings?) {
        if (!finished.compareAndSet(false, true)) return
        val ended = System.currentTimeMillis()
        engine.emit { sink ->
            sink.send(Envelope(Envelope.CALL_FAILED, CallFailed(callId, kind, message.take(500), timings, ended).encode()))
        }
    }

    private fun skipType(ct: String, size: Long): Boolean {
        val t = ct.lowercase()
        if (t.startsWith("image/") || t.startsWith("video/") || t.startsWith("audio/") || t.startsWith("font/")) return true
        if (t.startsWith("application/octet-stream") || t.startsWith("application/zip") || t.startsWith("application/pdf") ||
            t.startsWith("application/x-protobuf-stream")) return size < 0 || size > 64 * 1024 || size == 0L
        return false
    }
}

/** Process-wide state the cheap checks read. */
object Runtime {
    @Volatile @JvmField var engine: Engine? = null

    val live: Live? get() = engine?.live
}
