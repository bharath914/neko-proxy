package dev.proxy.inspector.internal

import dev.proxy.inspector.testing.TestCore
import dev.proxy.inspector.testing.TestPlatform
import java.lang.management.ManagementFactory
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class EngineTest {
    private val closers = mutableListOf<AutoCloseable>()

    @AfterTest
    fun cleanup() {
        Bootstrap.stop()
        closers.forEach { runCatching { it.close() } }
    }

    private fun <T : AutoCloseable> T.track(): T = also { closers += it }

    private fun waitUntil(what: String, timeoutMs: Long = 8_000, cond: () -> Boolean) {
        val end = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < end) { if (cond()) return; Thread.sleep(20) }
        throw AssertionError("timed out waiting for $what")
    }

    private fun call(url: String = "https://api.shop.test/v1/me", body: String? = null, response: String = """{"ok":true}""") {
        val h = assertNotNull(Recorder.begin(CallInfo("GET", url, listOf("Accept" to "application/json", "Authorization" to "Bearer abc"), "okhttp", name = "Profile")))
        body?.let { val b = it.toByteArray(); h.onRequestBody(b, 0, b.size) }
        h.responseStarted(200, listOf("Content-Type" to "application/json"), "h2", response.length.toLong())
        val r = response.toByteArray(); h.onResponseBody(r, 0, r.size)
        h.complete(Timings(2, 0, -1, 1, 20, 3), false)
    }

    @Test
    fun pairsStreamsAndReconnectsWithTheStoredToken() {
        val core = TestCore.start("--auto-allow").track()
        val platform = TestPlatform(ports = { listOf(core.port) })
        Bootstrap.start(platform)
        waitUntil("capture to turn on") { Recorder.active() }

        // The code the SDK logged is the one the desktop recorded.
        val prompt = core.ofType("pair_prompt").single()
        val logged = platform.logs.first { "Pairing code" in it }
        assertTrue(Pairing.spaced(prompt.getString("code")) in logged, logged)
        assertTrue(platform.notices.single().contains(Pairing.spaced(prompt.getString("code"))))

        call(body = """{"password":"hunter2","note":"hi"}""")
        val lines = core.waitFor("the flow") { l -> l.any { it.optString("type") == "flow" } }
        val flow = lines.first { it.optString("type") == "flow" }
        assertEquals("https://api.shop.test/v1/me", flow.getString("url"))
        assertEquals("Profile", flow.getJSONObject("sdk").getString("name"))
        assertEquals("okhttp", flow.getJSONObject("sdk").getString("client_stack"))
        // Redacted on the device, before it was sent.
        val reqHeaders = flow.getJSONArray("request_headers")
        assertEquals("[redacted]", (0 until reqHeaders.length()).map { reqHeaders.getJSONArray(it) }.first { it.getString(0) == "Authorization" }.getString(1))
        assertEquals("""{"password":"[redacted]","note":"hi"}""", flow.getJSONObject("request_body").getString("data_text"))
        assertTrue(flow.getJSONObject("sdk").getBoolean("request_redacted"))
        assertEquals("""{"ok":true}""", flow.getJSONObject("response_body").getString("data_text"))

        // Restart the SDK: the stored token is used, so no second prompt appears.
        Bootstrap.stop()
        val again = TestPlatform(storageDir = platform.storageDir, ports = { listOf(core.port) })
        Bootstrap.start(again)
        waitUntil("reconnect") { Recorder.active() }
        assertEquals(1, core.ofType("pair_prompt").size)
        assertEquals(2, core.ofType("session_start").size)
    }

    @Test
    fun forgettingThePairingAsksAgain() {
        val core = TestCore.start("--auto-allow").track()
        val platform = TestPlatform(ports = { listOf(core.port) })
        Bootstrap.start(platform)
        waitUntil("capture") { Recorder.active() }
        Bootstrap.stop()
        PairingStore(platform.storageDir).clear()
        Bootstrap.start(TestPlatform(storageDir = platform.storageDir, ports = { listOf(core.port) }))
        waitUntil("second pairing") { core.ofType("pair_prompt").size == 2 && Recorder.active() }
    }

    @Test
    fun aDifferentCertificateIsRefusedAndNoDataIsSent() {
        val first = TestCore.start("--auto-allow").track()
        val platform = TestPlatform(ports = { listOf(first.port) })
        Bootstrap.start(platform)
        waitUntil("capture") { Recorder.active() }
        Bootstrap.stop()
        val port = first.port
        first.close()

        // Another desktop (new certificate) answers at the same address.
        val impostor = TestCore.start("--auto-allow", port = port).track()
        val again = TestPlatform(storageDir = platform.storageDir, ports = { listOf(port) })
        Bootstrap.start(again)
        waitUntil("the mismatch to be logged", 12_000) { again.logs.any { "different certificate" in it } }
        assertFalse(Recorder.active())
        assertTrue(impostor.ofType("session_start").isEmpty(), "no session reached the other desktop")
        assertTrue(impostor.lines().isEmpty())
    }

    @Test
    fun whenNothingIsConnectedACallDoesNoCaptureWork() {
        assertNull(Runtime.engine)
        assertFalse(Recorder.active())
        val info = CallInfo("GET", "https://x.test/", listOf("A" to "b"), "okhttp")
        repeat(20_000) { Recorder.begin(info) } // warm up
        val mx = ManagementFactory.getThreadMXBean() as com.sun.management.ThreadMXBean
        val tid = Thread.currentThread().id
        val before = mx.getThreadAllocatedBytes(tid)
        repeat(100_000) { Recorder.begin(info); Recorder.active() }
        val allocated = mx.getThreadAllocatedBytes(tid) - before
        // Under one byte per call: allocation-free apart from JIT and profiler noise.
        assertTrue(allocated < 100_000, "allocated $allocated bytes over 100,000 disabled calls")
    }

    @Test
    fun captureOffMeansNoCaptureWork() {
        val core = TestCore.start("--auto-allow", "--config", cfgFile("""{"capture_enabled": false}""")).track()
        Bootstrap.start(TestPlatform(ports = { listOf(core.port) }))
        waitUntil("session") { core.ofType("session_start").isNotEmpty() }
        Thread.sleep(200)
        assertFalse(Recorder.active())
        assertNull(Recorder.begin(CallInfo("GET", "https://x.test/", emptyList(), "okhttp")))
    }

    @Test
    fun hostRulesAndStrictRedactionFromTheDesktopApply() {
        val core = TestCore.start("--auto-allow", "--config", cfgFile("""{"host_deny": ["ads.test"], "redaction": "strict"}""")).track()
        Bootstrap.start(TestPlatform(ports = { listOf(core.port) }))
        waitUntil("capture") { Recorder.active() }
        assertNull(Recorder.begin(CallInfo("GET", "https://ads.test/x", emptyList(), "okhttp")))
        call(body = """{"a":1}""")
        val flow = core.waitFor("flow") { l -> l.any { it.optString("type") == "flow" } }.first { it.optString("type") == "flow" }
        assertEquals("", flow.getJSONObject("request_body").optString("data_text"), "strict sends no bodies")
        val hdrs = flow.getJSONArray("request_headers")
        val accept = (0 until hdrs.length()).map { hdrs.getJSONArray(it) }.first { it.getString(0) == "Accept" }
        assertEquals("application/json", accept.getString(1), "safe headers stay")
    }

    @Test
    fun bigBodiesStayOnTheDeviceUntilTheDesktopAsks() {
        val core = TestCore.start("--auto-allow", "--fetch-lazy").track()
        Bootstrap.start(TestPlatform(ports = { listOf(core.port) }))
        waitUntil("capture") { Recorder.active() }
        val big = "x".repeat(300_000)
        call(response = big)
        // The flow arrives without the body, then --fetch-lazy asks for it and it comes in chunks.
        val lines = core.waitFor("lazy fetch") { l -> l.count { it.optString("type") == "body_fetch_result" } == 1 }
        val flow = lines.first { it.optString("type") == "flow" }
        assertEquals(1, flow.getJSONObject("sdk").getInt("response_body_state"), "marked as kept on the device")
        core.waitFor("chunks") { l -> l.any { it.optString("type") == "body_chunk" && it.optBoolean("last") } }
    }

    private fun cfgFile(json: String): String =
        java.io.File.createTempFile("cfg", ".json").also { it.writeText(json); it.deleteOnExit() }.absolutePath

    @Test
    fun ringEvictsTheOldestWithinItsBudget() {
        val ring = BodyRing(budget = 100)
        ring.put("a", 1, ByteArray(60)); ring.put("b", 1, ByteArray(60))
        assertNull(ring.get("a", 1)); assertNotNull(ring.get("b", 1))
        assertFalse(ring.put("huge", 1, ByteArray(101)))
        assertTrue(ring.size() <= 100)
    }

    @Test
    fun queueOverflowDropsTheOldestAndCountsIt() {
        val q = DropOldestQueue<Int>(3)
        (1..5).forEach { q.offer(it) }
        assertEquals(2, q.dropped())
        assertEquals(listOf(3, 4, 5), listOf(q.poll(1), q.poll(1), q.poll(1)))
        assertNull(q.poll(1))
    }
}
