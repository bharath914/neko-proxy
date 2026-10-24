package dev.nekoproxy.inspector.internal

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull

/** Decodes the golden frames written by the Rust test `core/crates/sdk/tests/fixtures.rs`. */
class WireTest {
    private class Frame(val dir: String, val name: String, val bytes: ByteArray)

    private val text = File(System.getProperty("sdk.fixtures")).readText()

    private fun section(name: String): List<Frame> {
        val start = text.indexOf("\"$name\": [")
        val end = text.indexOf("]", text.indexOf("\"hex\"", start).let { text.lastIndexOf("}", text.indexOf("]\n", start)) })
        val body = text.substring(start, text.indexOf("\n  ]", start))
        return Regex("\"dir\": \"(\\w+)\",\\s+\"name\": \"(\\w+)\",\\s+\"hex\": \"([0-9a-f]+)\"")
            .findAll(body).map { Frame(it.groupValues[1], it.groupValues[2], it.groupValues[3].hexToBytes()) }.toList()
            .also { assertNotNull(end); assertEquals(true, it.isNotEmpty()) }
    }

    private fun field(name: String) = Regex("\"$name\": \"([^\"]*)\"").find(text)!!.groupValues[1]

    private fun decode(f: Frame): Envelope = assertNotNull(Frames.read(ByteArrayInputStream(f.bytes)))

    private val token = field("token_hex").hexToBytes()
    private val hello = Hello(
        sdkName = "android", sdkVersion = "0.1.0", appId = "com.demo.shop", appName = "Shop", appVersion = "1.2.3",
        build = "42", deviceInstallId = "c0ffee00-1111-4222-8333-444455556666", deviceName = "Pixel 8",
        deviceModel = "Google Pixel 8", osVersion = "Android 15", deviceTimeMs = 1_800_000_000_000, connection = "usb",
    )

    private fun frameBytes(env: Envelope) = ByteArrayOutputStream().also { Frames.write(it, env) }.toByteArray()

    @Test
    fun pairingCodeMatchesTheRustKnownAnswer() {
        val fp = ByteArray(32) { it.toByte() }
        val cn = ByteArray(32) { (32 + it).toByte() }
        val sn = ByteArray(32) { (64 + it).toByte() }
        assertEquals("730832", Pairing.code(fp, cn, sn))
        assertEquals(field("code"), Pairing.code(field("fingerprint_hex").hexToBytes(), field("client_nonce_hex").hexToBytes(), field("server_nonce_hex").hexToBytes()))
    }

    @Test
    fun clientMessagesEncodeToTheSameBytesAsTheRustSide() {
        val fresh = section("fresh_pairing").associateBy { it.name }
        assertContentEquals(fresh.getValue("hello").bytes, frameBytes(Envelope(Envelope.HELLO, hello.encode())))
        val nonce = field("client_nonce_hex").hexToBytes()
        assertContentEquals(
            fresh.getValue("pair_request").bytes,
            frameBytes(Envelope(Envelope.PAIR_REQUEST, PairRequest(Pairing.commit(nonce)).encode())),
        )
        assertContentEquals(fresh.getValue("pair_reveal").bytes, frameBytes(Envelope(Envelope.PAIR_REVEAL, PairReveal(nonce).encode())))

        val re = section("reconnect").associateBy { it.name }
        assertContentEquals(re.getValue("hello").bytes, frameBytes(Envelope(Envelope.HELLO, hello.copy(token = token).encode())))
        val id = "0191a2b3-0000-7000-8000-000000000001"
        val start = CallStart(
            callId = id, startedAtMs = 1_800_000_001_000, method = "POST", url = "https://api.demo-shop.test/v1/cart/7/coupon",
            headers = listOf(Header("Accept", "application/json")), httpVersion = "h2", clientStack = "okhttp",
            routeTemplate = "/v1/cart/{id}/coupon", name = "Apply coupon", owner = "payments", screen = "CartActivity",
            requestBodySize = 19,
        )
        assertContentEquals(re.getValue("call_start").bytes, frameBytes(Envelope(Envelope.CALL_START, start.encode())))
        assertContentEquals(
            re.getValue("body_chunk").bytes,
            frameBytes(Envelope(Envelope.BODY_CHUNK, BodyChunk(id, Part.REQUEST, 0, """{"code":"SPRING25"}""".toByteArray(), last = true).encode())),
        )
        assertContentEquals(
            re.getValue("response_start").bytes,
            frameBytes(Envelope(Envelope.RESPONSE_START, ResponseStart(id, 200, listOf(Header("Content-Type", "application/json")), "h2", 17).encode())),
        )
        val complete = CallComplete(
            id, 1_800_000_001_084, Timings(0, 0, -1, 1, 60, 3), false,
            responseBodyAvailable = true, requestBodyAvailable = true,
        )
        assertContentEquals(re.getValue("call_complete").bytes, frameBytes(Envelope(Envelope.CALL_COMPLETE, complete.encode())))
    }

    @Test
    fun serverMessagesDecode() {
        val fresh = section("fresh_pairing").associateBy { it.name }
        val ack = HelloAck.decode(decode(fresh.getValue("hello_ack")).body)
        assertEquals(AckState.PAIRING_REQUIRED, ack.state)
        assertEquals("fedora", ack.desktopName)
        val ch = PairChallenge.decode(decode(fresh.getValue("pair_challenge")).body)
        assertContentEquals(field("server_nonce_hex").hexToBytes(), ch.serverNonce)
        assertEquals(60, ch.timeoutS)
        val res = PairResult.decode(decode(fresh.getValue("pair_result")).body)
        assertEquals(true, res.allowed)
        assertContentEquals(token, res.token)
        val cfg = Config.decode(decode(fresh.getValue("config")).body)
        assertEquals(Config(), cfg)

        val re = section("reconnect").associateBy { it.name }
        assertEquals(AckState.AUTHENTICATED, HelloAck.decode(decode(re.getValue("hello_ack")).body).state)
    }

    @Test
    fun everySectionDecodesInOrderAsEnvelopes() {
        for (s in listOf("fresh_pairing", "reconnect")) for (f in section(s)) assertNotNull(decode(f))
    }

    @Test
    fun negativeTimingsRoundTrip() {
        val t = Timings(dns = -1, connect = 0, tls = 30, requestSent = 1, ttfb = -1, download = 2)
        assertEquals(t.copy(connect = 0), Timings.decode(t.encode()).copy(connect = 0))
    }

    @Test
    fun malformedInputIsRejectedNotCrashed() {
        assertFailsWith<ProtoException> { Frames.read(ByteArrayInputStream(byteArrayOf(0, 0, 0, 0))) }
        assertFailsWith<ProtoException> { Frames.read(ByteArrayInputStream(byteArrayOf(0x7f, 0, 0, 0))) }
        assertFailsWith<ProtoException> { Envelope.decode(byteArrayOf(0x0a, 0x7f, 1)) }
        assertFailsWith<ProtoException> { Envelope.decode(ByteArray(0)) }
        assertEquals(null, Frames.read(ByteArrayInputStream(ByteArray(0))))
        // Unknown fields are skipped, so a newer peer stays readable.
        val w = ProtoWriter().apply { int(99, 5); string(1, "a"); string(2, "b") }.toByteArray()
        assertEquals(Header("a", "b"), Header.decode(w))
    }
}
