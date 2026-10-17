package dev.proxy.inspector.internal

/** Messages of `proto/sdk.proto`, protocol version 1. Field numbers must match that file. Public only so sibling artifacts can use them; not part of the supported API. */

const val PROTOCOL_VERSION = 1

object Part { const val REQUEST = 0; const val RESPONSE = 1 }
object Encoding { const val NONE = 0; const val DEFLATE = 1 }

object AckState {
    const val AUTHENTICATED = 0
    const val PAIRING_REQUIRED = 1
    const val DENIED = 2
    const val INGEST_DISABLED = 3
}

object ErrorCode {
    const val OTHER = 0
    const val PROTOCOL_VIOLATION = 1
    const val UNSUPPORTED_VERSION = 2
    const val RATE_LIMITED = 3
    const val TOO_MANY_SESSIONS = 4
    const val FRAME_TOO_LARGE = 5
}

object RedactionLevel { const val DEFAULT = 0; const val STRICT = 1; const val NONE = 2 }

data class Header(val name: String, val value: String) {
    fun encode() = ProtoWriter().apply { string(1, name); string(2, value) }.toByteArray()

    companion object {
        fun decode(b: ByteArray): Header {
            val r = ProtoReader(b)
            var n = ""; var v = ""
            while (r.next()) when (r.field) { 1 -> n = r.string(); 2 -> v = r.string(); else -> r.skip() }
            return Header(n, v)
        }
    }
}

data class Timings(
    val dns: Int = -1, val connect: Int = -1, val tls: Int = -1,
    val requestSent: Int = -1, val ttfb: Int = -1, val download: Int = -1,
) {
    fun encode() = ProtoWriter().apply {
        int(1, dns); int(2, connect); int(3, tls); int(4, requestSent); int(5, ttfb); int(6, download)
    }.toByteArray()

    companion object {
        fun decode(b: ByteArray): Timings {
            val r = ProtoReader(b)
            var t = Timings(0, 0, 0, 0, 0, 0)
            while (r.next()) t = when (r.field) {
                1 -> t.copy(dns = r.int()); 2 -> t.copy(connect = r.int()); 3 -> t.copy(tls = r.int())
                4 -> t.copy(requestSent = r.int()); 5 -> t.copy(ttfb = r.int()); 6 -> t.copy(download = r.int())
                else -> { r.skip(); t }
            }
            return t
        }
    }
}

data class Hello(
    val protocolVersion: Int = PROTOCOL_VERSION,
    val sdkName: String = "", val sdkVersion: String = "",
    val appId: String = "", val appName: String = "", val appVersion: String = "", val build: String = "",
    val deviceInstallId: String = "", val deviceName: String = "", val deviceModel: String = "",
    val osVersion: String = "", val deviceTimeMs: Long = 0, val token: ByteArray = ByteArray(0),
    val connection: String = "",
) {
    fun encode() = ProtoWriter().apply {
        int(1, protocolVersion); string(2, sdkName); string(3, sdkVersion); string(4, appId); string(5, appName)
        string(6, appVersion); string(7, build); string(8, deviceInstallId); string(9, deviceName)
        string(10, deviceModel); string(11, osVersion); int(12, deviceTimeMs); bytes(13, token); string(14, connection)
    }.toByteArray()
}

data class HelloAck(
    val state: Int = 0, val desktopName: String = "", val sessionId: String = "",
    val serverTimeMs: Long = 0, val retryAfterS: Int = 0, val message: String = "",
) {
    companion object {
        fun decode(b: ByteArray): HelloAck {
            val r = ProtoReader(b)
            var a = HelloAck()
            while (r.next()) a = when (r.field) {
                1 -> a.copy(state = r.int()); 2 -> a.copy(desktopName = r.string()); 3 -> a.copy(sessionId = r.string())
                4 -> a.copy(serverTimeMs = r.varint()); 5 -> a.copy(retryAfterS = r.int()); 6 -> a.copy(message = r.string())
                else -> { r.skip(); a }
            }
            return a
        }
    }
}

class PairRequest(val commit: ByteArray) {
    fun encode() = ProtoWriter().apply { bytes(1, commit) }.toByteArray()
}

class PairChallenge(val serverNonce: ByteArray, val timeoutS: Int) {
    companion object {
        fun decode(b: ByteArray): PairChallenge {
            val r = ProtoReader(b)
            var n = ByteArray(0); var t = 0
            while (r.next()) when (r.field) { 1 -> n = r.bytes(); 2 -> t = r.int(); else -> r.skip() }
            return PairChallenge(n, t)
        }
    }
}

class PairReveal(val clientNonce: ByteArray) {
    fun encode() = ProtoWriter().apply { bytes(1, clientNonce) }.toByteArray()
}

class PairResult(val allowed: Boolean, val token: ByteArray, val message: String) {
    companion object {
        fun decode(b: ByteArray): PairResult {
            val r = ProtoReader(b)
            var a = false; var t = ByteArray(0); var m = ""
            while (r.next()) when (r.field) { 1 -> a = r.bool(); 2 -> t = r.bytes(); 3 -> m = r.string(); else -> r.skip() }
            return PairResult(a, t, m)
        }
    }
}

data class CallStart(
    val callId: String, val startedAtMs: Long, val method: String, val url: String,
    val headers: List<Header> = emptyList(), val httpVersion: String = "", val clientStack: String = "",
    val routeTemplate: String = "", val name: String = "", val owner: String = "", val tag: String = "",
    val screen: String = "", val requestBodyTruncated: Boolean = false, val requestBodySize: Long = 0,
) {
    fun encode() = ProtoWriter().apply {
        string(1, callId); int(2, startedAtMs); string(3, method); string(4, url)
        headers.forEach { message(5, it.encode()) }
        string(6, httpVersion); string(7, clientStack); string(8, routeTemplate); string(9, name)
        string(10, owner); string(11, tag); string(12, screen); bool(13, requestBodyTruncated); int(14, requestBodySize)
    }.toByteArray()
}

class BodyChunk(
    val callId: String, val part: Int, val seq: Int, val data: ByteArray,
    val encoding: Int = Encoding.NONE, val last: Boolean = false, val truncated: Boolean = false,
) {
    fun encode() = ProtoWriter().apply {
        string(1, callId); int(2, part); int(3, seq); bytes(4, data); int(5, encoding); bool(6, last); bool(7, truncated)
    }.toByteArray()
}

class ResponseStart(
    val callId: String, val status: Int, val headers: List<Header>, val httpVersion: String, val responseBodySize: Long,
) {
    fun encode() = ProtoWriter().apply {
        string(1, callId); int(2, status); headers.forEach { message(3, it.encode()) }
        string(4, httpVersion); int(5, responseBodySize)
    }.toByteArray()
}

class CallComplete(
    val callId: String, val endedAtMs: Long, val timings: Timings?, val fromCache: Boolean,
    val responseBodyAvailable: Boolean, val requestBodyAvailable: Boolean,
    val requestRedacted: Boolean = false, val responseRedacted: Boolean = false,
    val responseBodySkipped: Boolean = false, val requestBodySkipped: Boolean = false,
    val responseBodySize: Long = 0, val requestBodySize: Long = 0,
) {
    fun encode() = ProtoWriter().apply {
        string(1, callId); int(2, endedAtMs); timings?.let { message(3, it.encode()) }; bool(4, fromCache)
        bool(5, responseBodyAvailable); bool(6, requestBodyAvailable); bool(7, requestRedacted)
        bool(8, responseRedacted); bool(9, responseBodySkipped); bool(10, requestBodySkipped)
        int(11, responseBodySize); int(12, requestBodySize)
    }.toByteArray()
}

class CallFailed(
    val callId: String, val kind: String, val message: String, val timings: Timings?, val endedAtMs: Long,
) {
    fun encode() = ProtoWriter().apply {
        string(1, callId); string(2, kind); string(3, message); timings?.let { message(4, it.encode()) }; int(5, endedAtMs)
    }.toByteArray()
}

/** Desktop to SDK settings for one device and app. */
data class Config(
    val captureEnabled: Boolean = true,
    val hostAllow: List<String> = emptyList(),
    val hostDeny: List<String> = emptyList(),
    val bodyCapBytes: Int = 1 shl 20,
    val eagerBodyBytes: Int = 64 shl 10,
    val redaction: Int = RedactionLevel.DEFAULT,
    val extraRedactKeys: List<String> = emptyList(),
) {
    fun encode() = ProtoWriter().apply {
        bool(1, captureEnabled); hostAllow.forEach { string(2, it) }; hostDeny.forEach { string(3, it) }
        int(4, bodyCapBytes); int(5, eagerBodyBytes); int(6, redaction); extraRedactKeys.forEach { string(7, it) }
    }.toByteArray()

    companion object {
        fun decode(b: ByteArray): Config {
            val r = ProtoReader(b)
            val allow = ArrayList<String>(); val deny = ArrayList<String>(); val keys = ArrayList<String>()
            var c = Config(captureEnabled = false, bodyCapBytes = 0, eagerBodyBytes = 0)
            while (r.next()) c = when (r.field) {
                1 -> c.copy(captureEnabled = r.bool()); 2 -> { allow += r.string(); c }; 3 -> { deny += r.string(); c }
                4 -> c.copy(bodyCapBytes = r.int()); 5 -> c.copy(eagerBodyBytes = r.int()); 6 -> c.copy(redaction = r.int())
                7 -> { keys += r.string(); c }
                else -> { r.skip(); c }
            }
            return c.copy(hostAllow = allow, hostDeny = deny, extraRedactKeys = keys)
        }
    }
}

class BodyFetch(val callId: String, val part: Int) {
    companion object {
        fun decode(b: ByteArray): BodyFetch {
            val r = ProtoReader(b)
            var c = ""; var p = 0
            while (r.next()) when (r.field) { 1 -> c = r.string(); 2 -> p = r.int(); else -> r.skip() }
            return BodyFetch(c, p)
        }
    }
}

class BodyFetchResult(val callId: String, val part: Int, val available: Boolean) {
    fun encode() = ProtoWriter().apply { string(1, callId); int(2, part); bool(3, available) }.toByteArray()
}

class Ping(val timeMs: Long) {
    fun encode() = ProtoWriter().apply { int(1, timeMs) }.toByteArray()
    companion object {
        fun decode(b: ByteArray): Ping {
            val r = ProtoReader(b); var t = 0L
            while (r.next()) if (r.field == 1) t = r.varint() else r.skip()
            return Ping(t)
        }
    }
}

class Pong(val echoMs: Long, val timeMs: Long) {
    fun encode() = ProtoWriter().apply { int(1, echoMs); int(2, timeMs) }.toByteArray()
    companion object {
        fun decode(b: ByteArray): Pong {
            val r = ProtoReader(b); var e = 0L; var t = 0L
            while (r.next()) when (r.field) { 1 -> e = r.varint(); 2 -> t = r.varint(); else -> r.skip() }
            return Pong(e, t)
        }
    }
}

class WireError(val code: Int, val message: String, val minVersion: Int, val maxVersion: Int) {
    companion object {
        fun decode(b: ByteArray): WireError {
            val r = ProtoReader(b)
            var c = 0; var m = ""; var lo = 0; var hi = 0
            while (r.next()) when (r.field) { 1 -> c = r.int(); 2 -> m = r.string(); 3 -> lo = r.int(); 4 -> hi = r.int(); else -> r.skip() }
            return WireError(c, m, lo, hi)
        }
    }
}

/** One frame's worth: which field of the Envelope's oneof is set, and its bytes. */
class Envelope(val kind: Int, val body: ByteArray) {
    fun encode() = ProtoWriter().apply { message(kind, body) }.toByteArray()

    companion object {
        const val HELLO = 1; const val HELLO_ACK = 2; const val PAIR_REQUEST = 3; const val PAIR_CHALLENGE = 4
        const val PAIR_REVEAL = 5; const val PAIR_RESULT = 6; const val CONFIG = 7; const val CALL_START = 8
        const val BODY_CHUNK = 9; const val RESPONSE_START = 10; const val CALL_COMPLETE = 11; const val CALL_FAILED = 12
        const val BODY_FETCH = 13; const val BODY_FETCH_RESULT = 14; const val PING = 15; const val PONG = 16
        const val ERROR = 17

        fun decode(b: ByteArray): Envelope {
            val r = ProtoReader(b)
            var found: Envelope? = null
            while (r.next()) {
                if (r.wire == 2 && r.field in HELLO..ERROR) found = Envelope(r.field, r.bytes()) else r.skip()
            }
            return found ?: throw ProtoException("envelope has no message")
        }
    }
}
