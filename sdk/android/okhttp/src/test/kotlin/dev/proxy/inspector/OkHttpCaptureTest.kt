package dev.proxy.inspector

import dev.proxy.inspector.internal.Bootstrap
import dev.proxy.inspector.internal.Recorder
import dev.proxy.inspector.testing.TestCore
import dev.proxy.inspector.testing.TestPlatform
import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.EventListener
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.ResponseBody
import okio.Buffer
import okio.GzipSink
import okio.buffer
import org.json.JSONObject
import retrofit2.Retrofit
import retrofit2.http.Body
import retrofit2.http.DELETE
import retrofit2.http.GET
import retrofit2.http.HEAD
import retrofit2.http.HTTP
import retrofit2.http.OPTIONS
import retrofit2.http.PATCH
import retrofit2.http.POST
import retrofit2.http.PUT
import retrofit2.http.Path
import retrofit2.http.Query
import java.util.concurrent.TimeUnit
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class OkHttpCaptureTest {
    private lateinit var core: TestCore
    private lateinit var server: MockWebServer

    @BeforeTest
    fun setUp() {
        core = TestCore.start("--auto-allow")
        server = MockWebServer().also { it.start() }
        Bootstrap.start(TestPlatform(ports = { listOf(core.port) }))
        val end = System.currentTimeMillis() + 10_000
        while (!Recorder.active() && System.currentTimeMillis() < end) Thread.sleep(20)
        assertTrue(Recorder.active(), "the SDK did not connect")
    }

    @AfterTest
    fun tearDown() {
        Bootstrap.stop(); server.close(); core.close()
    }

    private fun client(block: OkHttpClient.Builder.() -> Unit = {}) = OkHttpClient.Builder()
        .addInterceptor(InspectorInterceptor())
        .eventListenerFactory(InspectorEventListener.Factory(null))
        .apply(block).build()

    private fun flows(n: Int): List<JSONObject> = core.waitFor("$n flows") { l -> l.count { it.optString("type") == "flow" } >= n }.filter { it.optString("type") == "flow" }

    private fun header(flow: JSONObject, key: String, response: Boolean = false): String? {
        val a = flow.getJSONArray(if (response) "response_headers" else "request_headers")
        return (0 until a.length()).map { a.getJSONArray(it) }.firstOrNull { it.getString(0).equals(key, true) }?.getString(1)
    }

    @Test
    fun theAppGetsTheBodyUnchangedAndTheDesktopGetsACopy() {
        server.enqueue(MockResponse.Builder().body("""{"id":7,"name":"Asha"}""").addHeader("Content-Type", "application/json").build())
        val res = client().newCall(Request.Builder().url(server.url("/v1/me")).header("Authorization", "Bearer t").build()).execute()
        assertEquals("""{"id":7,"name":"Asha"}""", res.body.string())
        val f = flows(1).single()
        assertEquals(200, f.getInt("status"))
        assertEquals("""{"id":7,"name":"Asha"}""", f.getJSONObject("response_body").getString("data_text"))
        assertEquals("[redacted]", header(f, "authorization"))
        assertEquals("okhttp", f.getJSONObject("sdk").getString("client_stack"))
        assertEquals("http/1.1", f.getString("protocol").lowercase())
    }

    @Test
    fun postBodiesAreCopiedWhileTheyAreWritten() {
        server.enqueue(MockResponse.Builder().body("ok").build())
        val body = """{"code":"SPRING25","password":"p"}""".toRequestBody("application/json".toMediaTypeOrNull())
        client().newCall(Request.Builder().url(server.url("/pay")).post(body).build()).execute().close()
        assertEquals("""{"code":"SPRING25","password":"p"}""", server.takeRequest().body!!.utf8(), "the server got the original body")
        val f = flows(1).single()
        assertEquals("""{"code":"SPRING25","password":"[redacted]"}""", f.getJSONObject("request_body").getString("data_text"))
    }

    @Test
    fun gzipResponsesAreSeenDecodedAsTheAppSeesThem() {
        val gz = Buffer().also { b -> GzipSink(b).buffer().use { it.writeUtf8("""{"gz":true}""") } }
        server.enqueue(MockResponse.Builder().body(gz).addHeader("Content-Encoding", "gzip").addHeader("Content-Type", "application/json").build())
        val res = client().newCall(Request.Builder().url(server.url("/g")).build()).execute()
        assertEquals("""{"gz":true}""", res.body.string())
        assertEquals("""{"gz":true}""", flows(1).single().getJSONObject("response_body").getString("data_text"))
    }

    @Test
    fun redirectsShowAsSeparateFlowsAndTheFinalOneCarriesTheResult() {
        server.enqueue(MockResponse.Builder().code(302).addHeader("Location", "/final").build())
        server.enqueue(MockResponse.Builder().body("done").build())
        client().newCall(Request.Builder().url(server.url("/start")).build()).execute().body.string()
        val fl = flows(2)
        assertTrue(fl.any { it.getInt("status") == 302 && it.getString("url").endsWith("/start") })
        assertTrue(fl.any { it.getInt("status") == 200 })
    }

    @Test
    fun failuresAreReportedAndRethrown() {
        server.enqueue(MockResponse.Builder().headersDelay(3, TimeUnit.SECONDS).build())
        val c = client { readTimeout(300, TimeUnit.MILLISECONDS); retryOnConnectionFailure(false) }
        val err = runCatching { c.newCall(Request.Builder().url(server.url("/slow")).build()).execute() }.exceptionOrNull()
        assertTrue(err is java.net.SocketTimeoutException, "$err")
        val f = flows(1).single()
        assertEquals(3, f.getInt("state"))
        assertTrue(f.getString("error").startsWith("timeout"), f.getString("error"))
    }

    @Test
    fun bigResponsesAreCappedForTheCopyButTheAppReadsAllOfIt() {
        val big = "y".repeat(2_500_000)
        server.enqueue(MockResponse.Builder().body(big).addHeader("Content-Type", "text/plain").build())
        val got = client().newCall(Request.Builder().url(server.url("/big")).build()).execute().body.string()
        assertEquals(big.length, got.length)
        val f = flows(1).single()
        assertEquals(2_500_000, f.getLong("response_size"))
        // Over the 1 MiB cap and the eager limit: kept on the device, not sent.
        assertEquals(1, f.getJSONObject("sdk").getInt("response_body_state"))
    }

    @Test
    fun imagesRecordSizeOnly() {
        server.enqueue(MockResponse.Builder().body(Buffer().write(ByteArray(200_000))).addHeader("Content-Type", "image/png").build())
        client().newCall(Request.Builder().url(server.url("/i.png")).build()).execute().body.bytes()
        val f = flows(1).single()
        assertEquals(2, f.getJSONObject("sdk").getInt("response_body_state"), "skipped by policy")
        assertEquals(200_000, f.getLong("response_size"))
    }

    @Test
    fun eventStreamsDeliverChunksAsTheyArrive() {
        val sse = (1..5).joinToString("") { "data: tick $it\n\n" }
        server.enqueue(MockResponse.Builder().body(sse).addHeader("Content-Type", "text/event-stream").build())
        val res = client().newCall(Request.Builder().url(server.url("/events")).build()).execute()
        assertEquals(sse, res.body.string())
        val f = flows(1).single()
        assertEquals(sse, f.getJSONObject("response_body").getString("data_text"))
    }

    @Test
    fun theAppsOwnEventListenerStillRuns() {
        val seen = java.util.concurrent.CopyOnWriteArrayList<String>()
        val mine = object : EventListener() {
            override fun callStart(call: okhttp3.Call) { seen += "start" }
            override fun responseBodyEnd(call: okhttp3.Call, byteCount: Long) { seen += "body:$byteCount" }
            override fun callEnd(call: okhttp3.Call) { seen += "end" }
        }
        val builder = OkHttpClient.Builder().eventListenerFactory { mine }
        InspectorOkHttp.install(builder)
        InspectorOkHttp.install(builder) // twice: still one interceptor
        assertEquals(1, builder.interceptors().count { it is InspectorInterceptor })
        server.enqueue(MockResponse.Builder().body("hello").build())
        builder.build().newCall(Request.Builder().url(server.url("/x")).build()).execute().body.string()
        assertEquals(listOf("start", "body:5", "end"), seen)
        val f = flows(1).single()
        val t = f.getJSONObject("sdk")
        assertEquals("okhttp", t.getString("client_stack"))
    }

    @Test
    fun withTheListenerPhaseTimingsArePresent() {
        server.enqueue(MockResponse.Builder().body("t").build())
        client().newCall(Request.Builder().url(server.url("/t")).build()).execute().body.string()
        val f = flows(1).single()
        assertTrue(f.getLong("duration_us") >= 0)
        val call = core.ofType("call_complete").single().getJSONObject("timings")
        assertTrue(call.getInt("ttfb") >= 0, "$call")
        assertTrue(call.getInt("download") >= 0, "$call")
    }

    @Test
    fun withNoDesktopConnectedNothingIsCapturedAndTheCallStillWorks() {
        Bootstrap.stop()
        assertTrue(!Recorder.active())
        server.enqueue(MockResponse.Builder().body("plain").build())
        assertEquals("plain", client().newCall(Request.Builder().url(server.url("/p")).build()).execute().body.string())
    }

    // ------------------------------------------------------------ Retrofit and @Inspect

    interface Api {
        @Inspect(name = "Apply coupon", owner = "payments", tag = "checkout")
        @POST("cart/{id}/coupon")
        fun apply(@Path("id") id: String, @Body body: RequestBody2): retrofit2.Call<ResponseBody>

        @GET("items") fun list(@Query("page") page: Int): retrofit2.Call<ResponseBody>
        @PUT("items/{id}") fun put(@Path("id") id: String, @Body b: RequestBody2): retrofit2.Call<ResponseBody>
        @PATCH("items/{id}") fun patch(@Path("id") id: String, @Body b: RequestBody2): retrofit2.Call<ResponseBody>
        @DELETE("items/{id}") fun delete(@Path("id") id: String): retrofit2.Call<ResponseBody>
        @HEAD("items/{id}") fun head(@Path("id") id: String): retrofit2.Call<Void>
        @OPTIONS("items") fun options(): retrofit2.Call<ResponseBody>
        @HTTP(method = "REPORT", path = "reports/{id}", hasBody = true) fun report(@Path("id") id: String, @Body b: RequestBody2): retrofit2.Call<ResponseBody>
        @GET("/absolute/path") fun absolute(): retrofit2.Call<ResponseBody>
        @GET("me") suspend fun me(): ResponseBody
    }

    @Inspect(owner = "growth")
    interface ClassLevel { @GET("rec") fun rec(): retrofit2.Call<ResponseBody> }

    private fun retrofit(prefix: String = "/v1/") = Retrofit.Builder().baseUrl(server.url(prefix)).client(client()).build()

    private fun template(fl: JSONObject) = core.ofType("call_start").first { it.getString("url") == fl.getString("url") }.getString("route_template")

    @Test
    fun retrofitRouteTemplatesAndInspectMetadata() {
        val api = retrofit().create(Api::class.java)
        repeat(5) { server.enqueue(MockResponse.Builder().body("{}").build()) }
        server.enqueue(MockResponse.Builder().build()) // HEAD: no body
        repeat(3) { server.enqueue(MockResponse.Builder().body("{}").build()) }
        api.apply("7", RequestBody2("{}")).execute()
        api.list(2).execute(); api.put("3", RequestBody2("{}")).execute(); api.patch("3", RequestBody2("{}")).execute()
        api.delete("3").execute(); api.head("3").execute(); api.options().execute(); api.report("9", RequestBody2("{}")).execute(); api.absolute().execute()
        val starts = core.waitFor("9 call starts") { l -> l.count { it.optString("type") == "call_start" } >= 9 }.filter { it.optString("type") == "call_start" }
        val byMethodPath = starts.associate { (it.getString("method") + " " + it.getString("url").substringAfter(server.port.toString())) to it.getString("route_template") }
        assertEquals("/v1/cart/{id}/coupon", byMethodPath["POST /v1/cart/7/coupon"])
        assertEquals("/v1/items", byMethodPath["GET /v1/items?page=2"])
        assertEquals("/v1/items/{id}", byMethodPath["PUT /v1/items/3"])
        assertEquals("/v1/items/{id}", byMethodPath["PATCH /v1/items/3"])
        assertEquals("/v1/items/{id}", byMethodPath["DELETE /v1/items/3"])
        assertEquals("/v1/items/{id}", byMethodPath["HEAD /v1/items/3"])
        assertEquals("/v1/items", byMethodPath["OPTIONS /v1/items"])
        assertEquals("/v1/reports/{id}", byMethodPath["REPORT /v1/reports/9"])
        assertEquals("/absolute/path", byMethodPath["GET /absolute/path"])
        val coupon = starts.first { it.getString("url").endsWith("/coupon") }
        assertEquals("Apply coupon", coupon.getString("name"))
        assertEquals("payments", coupon.getString("owner"))
        assertEquals("checkout", coupon.getString("tag"))
    }

    @Test
    fun suspendFunctionsAndClassLevelInspectWork() {
        val r = retrofit("/")
        server.enqueue(MockResponse.Builder().body("{}").build())
        server.enqueue(MockResponse.Builder().body("{}").build())
        runBlocking { r.create(Api::class.java).me().close() }
        r.create(ClassLevel::class.java).rec().execute()
        val starts = core.waitFor("starts") { l -> l.count { it.optString("type") == "call_start" } >= 2 }.filter { it.optString("type") == "call_start" }
        assertEquals("/me", starts.first { it.getString("url").endsWith("/me") }.getString("route_template"))
        assertEquals("growth", starts.first { it.getString("url").endsWith("/rec") }.getString("owner"))
    }

    @Test
    fun withoutRetrofitTheTemplateIsEmpty() {
        server.enqueue(MockResponse.Builder().body("x").build())
        client().newCall(Request.Builder().url(server.url("/plain")).build()).execute().body.string()
        assertEquals("", core.waitFor("start") { l -> l.any { it.optString("type") == "call_start" } }.first { it.optString("type") == "call_start" }.getString("route_template"))
        assertNull(RetrofitSupport.metadata(Request.Builder().url("http://x.test/").build()))
    }
}

typealias RequestBody2 = okhttp3.RequestBody
private fun RequestBody2(s: String): okhttp3.RequestBody = s.toRequestBody("application/json".toMediaTypeOrNull())
