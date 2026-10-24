package dev.nekoproxy.inspector

import dev.nekoproxy.inspector.internal.CallHandle
import dev.nekoproxy.inspector.internal.CallInfo
import dev.nekoproxy.inspector.internal.Recorder
import dev.nekoproxy.inspector.internal.Timings
import okhttp3.Interceptor
import okhttp3.MediaType
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.Response
import okhttp3.ResponseBody
import okio.Buffer
import okio.BufferedSink
import okio.BufferedSource
import okio.ForwardingSink
import okio.ForwardingSource
import okio.Source
import okio.buffer
import java.io.IOException

/**
 * Streams every call made through the client to the desktop inspector. Add it once:
 * `OkHttpClient.Builder().addInterceptor(InspectorInterceptor()).build()`. With the Gradle plugin
 * applied it is added for you.
 *
 * Bodies are copied as the app reads and writes them, never consumed or altered. With no desktop
 * connected the cost of a request is one volatile read.
 */
class InspectorInterceptor : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        // The disabled path: one volatile read, no allocation.
        if (!Recorder.active()) return chain.proceed(chain.request())
        val request = chain.request()
        val handle = Recorder.begin(describe(request)) ?: return chain.proceed(request)
        val call = chain.call()
        val timing = InspectorEventListener.timings[call]
        val begun = System.nanoTime()
        // If the app never reads the body, the end of the call still finishes it.
        timing?.finish = { handle.complete(finalTimings(timing, begun), timing.cacheHit) }

        val body = request.body
        val traced = if (body != null && !body.isDuplex()) request.newBuilder().method(request.method, TeeRequestBody(body, handle)).build() else request
        val response = try {
            chain.proceed(traced)
        } catch (e: Throwable) {
            val t = timing?.toTimings() ?: Timings()
            handle.fail(kindOf(e), e.message ?: e.javaClass.simpleName, t)
            throw e
        }

        val priors = generateSequence(response.priorResponse) { it.priorResponse }.toList().asReversed()
        if (priors.isNotEmpty()) redirectHops(priors, request, handle)

        val ct = response.body?.contentType()
        val len = response.body?.contentLength() ?: -1L
        handle.responseStarted(response.code, response.headers.map { it.first to it.second }, response.protocol.toString(), maxOf(len, 0))
        val noBody = response.body == null || response.code == 101 || request.method == "HEAD" || len == 0L
        if (noBody) {
            handle.complete(finalTimings(timing, begun), timing?.cacheHit == true)
            return response
        }
        // The body wrapper finishes the call when the app has read it all. The listener's callEnd fires
        // earlier (inside the last read), so it must not finish the call once a body is being copied.
        timing?.finish = null
        val traced2 = TeeResponseBody(response.body!!, handle) { handle.complete(finalTimings(timing, begun), timing?.cacheHit == true) }
        return response.newBuilder().body(traced2).build().also { ct?.let { } }
    }

    private fun describe(r: Request): CallInfo {
        val meta = RetrofitSupport.metadata(r)
        return CallInfo(
            method = r.method,
            url = r.url.toString(),
            headers = r.headers.map { it.first to it.second } + (r.body?.contentType()?.let { listOf("Content-Type" to it.toString()) }.orEmpty().filter { r.header("Content-Type") == null }),
            clientStack = "okhttp",
            routeTemplate = meta?.template.orEmpty(),
            name = meta?.name.orEmpty(),
            owner = meta?.owner.orEmpty(),
            tag = meta?.tag.orEmpty(),
            requestBodySize = r.body?.contentLength()?.coerceAtLeast(0) ?: 0,
        )
    }

    private fun RequestBody.isDuplex(): Boolean = try { isDuplex() } catch (e: Throwable) { false }

    /** Redirect hops are separate flows, as the proxy would show them. The first one matches the proxy's record of the call. */
    private fun redirectHops(priors: List<Response>, original: Request, final: CallHandle) {
        for (p in priors) {
            val h = Recorder.begin(CallInfo(p.request.method, p.request.url.toString(), p.request.headers.map { it.first to it.second }, "okhttp", tag = "redirect")) ?: return
            h.responseStarted(p.code, p.headers.map { it.first to it.second }, p.protocol.toString(), 0)
            h.complete(null, false)
        }
    }

    private fun finalTimings(t: CallTiming?, begunNs: Long): Timings {
        if (t == null) {
            // No event listener: only the total is known.
            val total = ((System.nanoTime() - begunNs) / 1_000_000L).toInt()
            return Timings(-1, -1, -1, -1, -1, total)
        }
        return t.toTimings()
    }

    private fun kindOf(e: Throwable): String = when (e) {
        is java.net.SocketTimeoutException, is java.io.InterruptedIOException -> if (e.message == "Canceled") "cancelled" else "timeout"
        is java.net.UnknownHostException -> "dns"
        is javax.net.ssl.SSLException -> "tls"
        is IOException -> if (e.message == "Canceled") "cancelled" else "io"
        else -> "other"
    }
}

/** Copies request bytes as OkHttp writes them. Everything else is forwarded unchanged. */
private class TeeRequestBody(private val inner: RequestBody, private val handle: CallHandle) : RequestBody() {
    override fun contentType(): MediaType? = inner.contentType()
    override fun contentLength(): Long = inner.contentLength()
    override fun isOneShot(): Boolean = inner.isOneShot()
    override fun isDuplex(): Boolean = inner.isDuplex()

    override fun writeTo(sink: BufferedSink) {
        handle.resetRequestBody() // a retry writes the body again
        val tee = object : ForwardingSink(sink) {
            override fun write(source: Buffer, byteCount: Long) {
                val copy = Buffer()
                source.copyTo(copy, 0, byteCount)
                val b = copy.readByteArray()
                handle.onRequestBody(b, 0, b.size)
                super.write(source, byteCount)
            }
        }.buffer()
        inner.writeTo(tee)
        tee.flush()
    }
}

/** Copies response bytes as the app reads them; finishes the call when the app reaches the end or closes the body. */
private class TeeResponseBody(
    private val inner: ResponseBody,
    private val handle: CallHandle,
    private val done: () -> Unit,
) : ResponseBody() {
    private val buffered: BufferedSource by lazy {
        object : ForwardingSource(inner.source()) {
            private var ended = false
            override fun read(sink: Buffer, byteCount: Long): Long {
                val before = sink.size
                val n = try { super.read(sink, byteCount) } catch (e: IOException) { finish(); throw e }
                if (n > 0) {
                    val copy = Buffer()
                    sink.copyTo(copy, before, n)
                    val b = copy.readByteArray()
                    handle.onResponseBody(b, 0, b.size)
                } else if (n < 0) finish()
                return n
            }
            override fun close() { try { super.close() } finally { finish() } }
            private fun finish() { if (!ended) { ended = true; done() } }
        }.buffer()
    }

    override fun contentType(): MediaType? = inner.contentType()
    override fun contentLength(): Long = inner.contentLength()
    override fun source(): BufferedSource = buffered
}
