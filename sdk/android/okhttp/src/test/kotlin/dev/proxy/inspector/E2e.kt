package dev.proxy.inspector

import dev.proxy.inspector.internal.Bootstrap
import dev.proxy.inspector.internal.Recorder
import dev.proxy.inspector.testing.TestPlatform
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.ResponseBody
import retrofit2.Retrofit
import retrofit2.http.GET
import retrofit2.http.Path
import java.net.InetSocketAddress
import java.net.Proxy
import java.util.concurrent.TimeUnit

interface E2eApi {
    @Inspect(name = "List items", owner = "catalog")
    @GET("json") fun items(): retrofit2.Call<ResponseBody>

    @GET("status/{code}") fun status(@Path("code") code: Int): retrofit2.Call<ResponseBody>
}

/**
 * `scripts/e2e-jvm.sh`: the real Android SDK engine (on the JVM) against a real `proxyd run --sdk`.
 * Prints PAIRING_CODE for the script, waits for someone to allow it, then makes calls, some of them also through the proxy.
 */
fun main(args: Array<String>) {
    val sdkPort = args[0].toInt()
    val base = args[1]
    val proxyPort = args[2].toInt()
    Bootstrap.start(object : TestPlatform(ports = { listOf(sdkPort) }) {
        override fun log(level: dev.proxy.inspector.internal.LogLevel, message: String) {
            super.log(level, message)
            if ("Pairing code" in message) println("PAIRING_CODE " + message.substringAfter("Pairing code ").take(7))
            System.out.flush()
        }
    })
    val end = System.currentTimeMillis() + 90_000
    while (!Recorder.active() && System.currentTimeMillis() < end) Thread.sleep(100)
    check(Recorder.active()) { "never connected" }
    println("CONNECTED")

    val direct = OkHttpClient.Builder().addInterceptor(InspectorInterceptor()).eventListenerFactory(InspectorEventListener.Factory(null)).build()
    val retrofit = Retrofit.Builder().baseUrl(base).client(direct).build().create(E2eApi::class.java)
    retrofit.items().execute().body()?.close()
    runCatching { retrofit.status(500).execute().body()?.close() }
    Inspector.setScreen("E2eScreen")
    direct.newCall(Request.Builder().url(base + "json/100k").build()).execute().body.bytes()

    // The same call seen by the SDK and by the proxy: the core should merge them into one flow.
    val viaProxy = OkHttpClient.Builder().addInterceptor(InspectorInterceptor())
        .proxy(Proxy(Proxy.Type.HTTP, InetSocketAddress("127.0.0.1", proxyPort))).build()
    viaProxy.newCall(Request.Builder().url(base + "json?merge=1").build()).execute().body.bytes()
    Thread.sleep(1500)
    println("DONE")
    Bootstrap.stop()
    System.exit(0)
}
