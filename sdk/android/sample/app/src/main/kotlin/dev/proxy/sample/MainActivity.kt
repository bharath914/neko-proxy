package dev.proxy.sample

import android.app.Activity
import android.os.Bundle
import android.util.Log
import android.widget.TextView
import dev.proxy.inspector.Inspector
import dev.proxy.inspector.inspect
import dev.proxy.samplelib.LibraryClient
import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.request.get
import io.ktor.client.request.prepareGet
import io.ktor.client.statement.bodyAsChannel
import io.ktor.client.statement.bodyAsText
import io.ktor.utils.io.readAvailable
import kotlinx.coroutines.runBlocking
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.RequestBody.Companion.toRequestBody
import retrofit2.Retrofit
import kotlin.concurrent.thread

/**
 * Makes one call of every kind, against the local test server (`sdk/test-server/server.py`).
 * The base URL comes from the `base` intent extra, default the emulator's host.
 *
 * Capture needs nothing here: no interceptor, no plugin install. The Gradle plugin did it.
 */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val label = TextView(this).apply { text = "Inspector sample: making calls…" }
        setContentView(label)
        val base = intent.getStringExtra("base") ?: "http://10.0.2.2:18080/"
        thread {
            runCatching { demo(base) }.onFailure { Log.e("InspectorSample", "demo failed", it) }
            runOnUiThread { label.text = "Done. Look at the desktop app." }
        }
    }

    private fun demo(base: String) = runBlocking {
        // Retrofit with @Inspect: name, owner and route template show on the desktop.
        val retrofit = Retrofit.Builder().baseUrl(base).client(OkHttpClient()).build()
        val api = retrofit.create(ShopApi::class.java)
        api.items().close()
        api.order("""{"item":7,"password":"hunter2"}""".toRequestBody("application/json".toMediaType())).close()
        runCatching { api.status(500).close() }

        // A client built inside a dependency.
        LibraryClient.fetch(base + "gzip")

        // Ktor on the OkHttp engine and on CIO. Calls are named with inspect(), a streaming response included.
        for ((name, engine) in listOf("okhttp" to OkHttp, "cio" to CIO)) {
            val client = HttpClient(engine)
            client.get(base + "json") { inspect(name = "Ktor $name") }.bodyAsText()
            client.prepareGet(base + "sse?n=5&ms=100") { inspect(name = "Ktor $name events") }.execute { resp ->
                val ch = resp.bodyAsChannel()
                val buf = ByteArray(1024)
                while (ch.readAvailable(buf, 0, buf.size) >= 0) { /* the app reads it all */ }
            }
            client.close()
        }
        Inspector.setScreen("SampleDemo")
        Log.i("InspectorSample", "demo finished")
    }

}
