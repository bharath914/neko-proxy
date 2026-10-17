package dev.proxy.inspector

import dev.proxy.inspector.internal.CallHandle
import dev.proxy.inspector.internal.CallInfo
import dev.proxy.inspector.internal.Recorder
import dev.proxy.inspector.internal.Timings
import io.ktor.client.HttpClient
import io.ktor.client.plugins.api.Send
import io.ktor.client.plugins.api.createClientPlugin
import io.ktor.client.request.HttpRequestBuilder
import io.ktor.client.plugins.api.ClientHook
import io.ktor.client.plugins.isSaved
import io.ktor.client.plugins.observer.wrapWithContent
import io.ktor.client.statement.HttpReceivePipeline
import io.ktor.util.pipeline.PipelineContext
import io.ktor.util.split
import io.ktor.utils.io.readRemaining
import kotlinx.io.readByteArray
import io.ktor.client.statement.HttpResponse
import io.ktor.http.content.OutgoingContent
import io.ktor.util.AttributeKey
import io.ktor.utils.io.InternalAPI
import io.ktor.utils.io.readAvailable
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch

/** Per-request metadata, shown on the desktop. Set with [inspect]. */
val InspectName = AttributeKey<String>("dev.proxy.inspector.name")
val InspectOwner = AttributeKey<String>("dev.proxy.inspector.owner")
val InspectTag = AttributeKey<String>("dev.proxy.inspector.tag")
val InspectRoute = AttributeKey<String>("dev.proxy.inspector.route")

/** Names this request for the desktop inspector, and optionally gives its route template. */
fun HttpRequestBuilder.inspect(name: String = "", owner: String = "", tag: String = "", route: String = "") {
    if (name.isNotEmpty()) attributes.put(InspectName, name)
    if (owner.isNotEmpty()) attributes.put(InspectOwner, owner)
    if (tag.isNotEmpty()) attributes.put(InspectTag, tag)
    if (route.isNotEmpty()) attributes.put(InspectRoute, route)
}

private val HandleKey = AttributeKey<CallHandle>("dev.proxy.inspector.call")
private val StartKey = AttributeKey<Long>("dev.proxy.inspector.start")
private val TtfbKey = AttributeKey<Long>("dev.proxy.inspector.ttfb")

/**
 * Streams every call made through the client to the desktop inspector:
 * `HttpClient(CIO) { install(InspectorPlugin) }`. Works with any engine.
 *
 * The response body is copied from a split of the body channel, so the app still receives all of
 * it, streaming included. Request bodies are copied when they are in memory; streamed uploads
 * record size only. With no desktop connected the cost is one volatile read per request.
 */
@OptIn(InternalAPI::class)
val InspectorPlugin = createClientPlugin("Inspector") {
    on(Send) { request ->
        if (!Recorder.active()) return@on proceed(request)
        val handle = Recorder.begin(describe(request)) ?: return@on proceed(request)
        val start = System.nanoTime()
        // The call object seen by the receive pipeline is a different instance; the request's
        // attributes are shared, so the handle travels there.
        request.attributes.put(HandleKey, handle)
        request.attributes.put(StartKey, start)
        (request.body as? OutgoingContent.ByteArrayContent)?.bytes()?.let { handle.onRequestBody(it, 0, it.size) }
        val call = try {
            proceed(request)
        } catch (e: Throwable) {
            handle.fail(kindOf(e), e.message ?: e.javaClass.simpleName, Timings(-1, -1, -1, -1, -1, ms(start)))
            throw e
        }
        call
    }

    // Split the body the way Ktor's own ResponseObserver does: one half goes to the app untouched,
    // the other is copied to the desktop as it arrives. Streaming responses keep streaming.
    on(AfterReceiveHook) { response ->
        val handle = response.call.request.attributes.getOrNull(HandleKey) ?: return@on
        // The receive pipeline can run before the Send hook returns, so this is where the response
        // is announced: always before any body bytes are copied.
        response.call.request.attributes.getOrNull(StartKey)?.let { response.call.request.attributes.put(TtfbKey, ms(it).toLong()) }
        handle.responseStarted(
            response.status.value, response.headers.entries().flatMap { (k, vs) -> vs.map { k to it } },
            response.version.toString(), response.headers["Content-Length"]?.toLongOrNull() ?: 0,
        )
        if (response.isSaved) {
            // Already read into memory by the app's code: copy it whole.
            val bytes = runCatching { response.rawContent.readRemaining().readByteArray() }.getOrDefault(ByteArray(0))
            handle.onResponseBody(bytes, 0, bytes.size)
            handle.complete(finish(response), false)
            proceedWith(response)
            return@on
        }
        val (side, appSide) = response.rawContent.split(response)
        val newResponse = response.call.wrapWithContent(appSide).response
        client.launch(start = CoroutineStart.UNDISPATCHED) {
            val buf = ByteArray(8192)
            try {
                while (true) {
                    val n = side.readAvailable(buf, 0, buf.size)
                    if (n < 0) break
                    if (n > 0) handle.onResponseBody(buf, 0, n)
                }
                handle.complete(finish(response), false)
            } catch (e: Throwable) {
                handle.fail("io", e.message ?: "body read failed", Timings(-1, -1, -1, -1, -1, -1))
            }
        }
        proceedWith(newResponse)
    }
}

private fun finish(response: HttpResponse): Timings {
    val start = response.call.request.attributes.getOrNull(StartKey) ?: return Timings()
    val ttfb = response.call.request.attributes.getOrNull(TtfbKey)?.toInt() ?: -1
    return Timings(-1, -1, -1, -1, ttfb, (ms(start) - ttfb).coerceAtLeast(0))
}

private object AfterReceiveHook : ClientHook<suspend AfterReceiveHook.Context.(HttpResponse) -> Unit> {
    class Context(private val context: PipelineContext<HttpResponse, Unit>) {
        suspend fun proceedWith(response: HttpResponse) = context.proceedWith(response)
    }

    override fun install(client: HttpClient, handler: suspend Context.(HttpResponse) -> Unit) {
        client.receivePipeline.intercept(HttpReceivePipeline.After) { handler(Context(this), subject) }
    }
}

private fun ms(startNs: Long): Int = ((System.nanoTime() - startNs) / 1_000_000L).toInt()

private fun describe(r: HttpRequestBuilder): CallInfo {
    val headers = r.headers.entries().flatMap { (k, vs) -> vs.map { k to it } }.toMutableList()
    val ct = r.body.let { (it as? OutgoingContent)?.contentType }
    if (ct != null && headers.none { it.first.equals("Content-Type", true) }) headers += "Content-Type" to ct.toString()
    val size = (r.body as? OutgoingContent)?.contentLength ?: 0
    return CallInfo(
        method = r.method.value, url = r.url.buildString(), headers = headers, clientStack = "ktor",
        routeTemplate = r.attributes.getOrNull(InspectRoute).orEmpty(), name = r.attributes.getOrNull(InspectName).orEmpty(),
        owner = r.attributes.getOrNull(InspectOwner).orEmpty(), tag = r.attributes.getOrNull(InspectTag).orEmpty(),
        requestBodySize = size,
    )
}

private fun kindOf(e: Throwable): String = when (e) {
    is kotlinx.coroutines.CancellationException -> "cancelled"
    is java.net.UnknownHostException -> "dns"
    is javax.net.ssl.SSLException -> "tls"
    is java.net.SocketTimeoutException -> "timeout"
    is java.io.IOException -> "io"
    else -> if (e.javaClass.simpleName.contains("Timeout")) "timeout" else "other"
}
