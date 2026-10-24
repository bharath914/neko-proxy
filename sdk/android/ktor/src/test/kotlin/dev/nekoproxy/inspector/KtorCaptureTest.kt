package dev.nekoproxy.inspector

import dev.nekoproxy.inspector.internal.Bootstrap
import dev.nekoproxy.inspector.internal.Recorder
import dev.nekoproxy.inspector.testing.TestCore
import dev.nekoproxy.inspector.testing.TestPlatform
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.engine.HttpClientEngineFactory
import io.ktor.client.engine.cio.CIO
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.request.get
import io.ktor.client.request.post
import io.ktor.client.request.prepareGet
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsChannel
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentType
import io.ktor.http.headersOf
import io.ktor.utils.io.readAvailable
import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import org.json.JSONObject
import java.util.concurrent.TimeUnit
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class KtorCaptureTest {
    private lateinit var core: TestCore
    private lateinit var server: MockWebServer

    @BeforeTest fun setUp() {
        core = TestCore.start("--auto-allow")
        server = MockWebServer().also { it.start() }
        Bootstrap.start(TestPlatform(ports = { listOf(core.port) }))
        val end = System.currentTimeMillis() + 10_000
        while (!Recorder.active() && System.currentTimeMillis() < end) Thread.sleep(20)
        assertTrue(Recorder.active())
    }

    @AfterTest fun tearDown() { Bootstrap.stop(); server.close(); core.close() }

    private fun flows(n: Int): List<JSONObject> =
        core.waitFor("$n flows") { l -> l.count { it.optString("type") == "flow" } >= n }.filter { it.optString("type") == "flow" }

    /** Waits off the runBlocking thread, which the plugin's body reader may need. */
    private suspend fun flowsS(n: Int) = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { flows(n) }

    private fun mock() = MockEngine { req ->
        when (req.url.encodedPath) {
            "/json" -> respond("""{"a":1}""", HttpStatusCode.OK, headersOf(HttpHeaders.ContentType, "application/json"))
            "/echo" -> respond((req.body as io.ktor.http.content.OutgoingContent.ByteArrayContent).bytes().decodeToString(), HttpStatusCode.Created, headersOf(HttpHeaders.ContentType, "text/plain"))
            else -> respond("nope", HttpStatusCode.NotFound)
        }
    }

    @Test
    fun mockEngineRequestResponseAndMetadata() = runBlocking {
        val client = HttpClient(mock()) { install(InspectorPlugin) }
        val text = client.post("https://api.test/echo") {
            contentType(ContentType.Application.Json)
            setBody("""{"password":"p","x":1}""")
            inspect(name = "Echo", owner = "qa", tag = "t1", route = "/echo")
        }.bodyAsText()
        assertEquals("""{"password":"p","x":1}""", text, "the app got the body")
        val f = flowsS(1).single()
        assertEquals(201, f.getInt("status"))
        assertEquals("""{"password":"[redacted]","x":1}""", f.getJSONObject("request_body").getString("data_text"))
        assertEquals("""{"password":"p","x":1}""", f.getJSONObject("response_body").getString("data_text").let { "$it" }.let { r -> if (r.contains("redacted")) r else r })
        val sdk = f.getJSONObject("sdk")
        assertEquals("ktor", sdk.getString("client_stack"))
        assertEquals("Echo", sdk.getString("name")); assertEquals("qa", sdk.getString("owner"))
        assertEquals("t1", sdk.getString("tag")); assertEquals("/echo", sdk.getString("route_template"))
    }

    @Test
    fun jsonResponsesAndErrors() = runBlocking {
        val client = HttpClient(mock()) { install(InspectorPlugin); expectSuccess = false }
        assertEquals("""{"a":1}""", client.get("https://api.test/json").body<String>())
        assertEquals(404, client.get("https://api.test/missing").status.value)
        val fl = flowsS(2)
        assertEquals(setOf(200, 404), fl.map { it.getInt("status") }.toSet())
    }

    @Test
    fun theFailureOfACallIsReportedAndRethrown() = runBlocking {
        val engine = MockEngine { throw java.net.ConnectException("refused") }
        val client = HttpClient(engine) { install(InspectorPlugin) }
        val err = runCatching { client.get("https://api.test/x") }.exceptionOrNull()
        assertTrue(err is java.net.ConnectException, "$err")
        val f = flowsS(1).single()
        assertEquals(3, f.getInt("state"))
        assertTrue(f.getString("error").startsWith("io"), f.getString("error"))
    }

    private fun realEngines(): List<Pair<String, HttpClientEngineFactory<*>>> = listOf("cio" to CIO, "okhttp" to OkHttp)

    @Test
    fun streamingResponsesArriveWholeOnBothRealEngines() {
        for ((name, engine) in realEngines()) {
            val before = core.lines().count { it.optString("type") == "flow" }
            val sse = (1..6).joinToString("") { "data: tick $it\n\n" }
            server.enqueue(MockResponse.Builder().body(sse).addHeader("Content-Type", "text/event-stream").bodyDelay(100, TimeUnit.MILLISECONDS).build())
            runBlocking {
                val client = HttpClient(engine) { install(InspectorPlugin) }
                val got = StringBuilder()
                client.prepareGet(server.url("/events").toString()).execute { resp ->
                    val ch = resp.bodyAsChannel()
                    val buf = ByteArray(16)
                    while (true) { val n = ch.readAvailable(buf, 0, buf.size); if (n < 0) break; got.append(String(buf, 0, n)) }
                }
                client.close()
                assertEquals(sse, got.toString(), "$name: the app must receive every byte")
            }
            val f = flows(before + 1).last()
            assertEquals(sse, f.getJSONObject("response_body").getString("data_text"), "$name copy")
            assertEquals("ktor", f.getJSONObject("sdk").getString("client_stack"))
        }
    }

    @Test
    fun ktorOnTheOkHttpEngineWithAnInstrumentedClientIsRecordedOnce() = runBlocking {
        server.enqueue(MockResponse.Builder().body("{}").addHeader("Content-Type", "application/json").build())
        val client = HttpClient(OkHttp) {
            engine { config { InspectorOkHttp.install(this) } } // as the Gradle plugin would
            install(InspectorPlugin)
        }
        client.get(server.url("/once").toString()) { inspect(name = "Once") }.bodyAsText()
        client.close()
        Thread.sleep(800)
        val fl = flowsS(1)
        assertEquals(1, fl.size, "one call, one flow")
        assertEquals("ktor", fl[0].getJSONObject("sdk").getString("client_stack"), "the Ktor record wins")
        assertEquals("Once", fl[0].getJSONObject("sdk").getString("name"))
    }

    @Test
    fun withNoDesktopNothingIsCaptured() = runBlocking {
        Bootstrap.stop()
        val client = HttpClient(mock()) { install(InspectorPlugin) }
        assertEquals("""{"a":1}""", client.get("https://api.test/json").bodyAsText())
    }
}
