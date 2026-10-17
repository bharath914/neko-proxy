package dev.proxy.inspector

import dev.proxy.inspector.internal.Timings
import okhttp3.Call
import okhttp3.Connection
import okhttp3.EventListener
import okhttp3.Handshake
import okhttp3.HttpUrl
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Proxy
import java.util.Collections
import java.util.WeakHashMap

/** What OkHttp's own events said about one call. Nanosecond timestamps, 0 when the phase did not happen. */
internal class CallTiming {
    @Volatile var start = System.nanoTime()
    @Volatile var dnsStart = 0L; @Volatile var dnsEnd = 0L
    @Volatile var connectStart = 0L; @Volatile var connectEnd = 0L
    @Volatile var tlsStart = 0L; @Volatile var tlsEnd = 0L
    @Volatile var reqStart = 0L; @Volatile var reqEnd = 0L
    @Volatile var resHeadersStart = 0L; @Volatile var resHeadersEnd = 0L
    @Volatile var resBodyEnd = 0L
    @Volatile var connectFailures = 0
    @Volatile var cacheHit = false
    @Volatile var finish: (() -> Unit)? = null

    private fun ms(a: Long, b: Long): Int = if (a == 0L || b == 0L || b < a) -1 else ((b - a) / 1_000_000L).toInt()

    fun toTimings(nowNs: Long = System.nanoTime()): Timings {
        val reused = connectStart == 0L && dnsStart == 0L
        val tls = ms(tlsStart, tlsEnd)
        val connect = if (reused) 0 else ms(connectStart, connectEnd).let { c -> if (c >= 0 && tls > 0) (c - tls).coerceAtLeast(0) else c }
        val bodyEnd = if (resBodyEnd != 0L) resBodyEnd else nowNs
        return Timings(
            dns = if (reused) -1 else ms(dnsStart, dnsEnd),
            connect = connect,
            tls = if (tlsStart == 0L) -1 else tls,
            requestSent = ms(reqStart, if (reqEnd != 0L) reqEnd else resHeadersStart),
            ttfb = ms(if (reqEnd != 0L) reqEnd else reqStart, resHeadersStart),
            download = ms(resHeadersEnd, bodyEnd),
        )
    }
}

/**
 * Collects timings from OkHttp's events and forwards every event to the app's own listener first,
 * so its behaviour never changes. Installed by the Gradle plugin, or by [InspectorOkHttp.install].
 */
internal class InspectorEventListener(private val delegate: EventListener?) : EventListener() {
    private fun t(call: Call) = timings.getOrPut(call) { CallTiming() }
    private inline fun mine(call: Call, f: CallTiming.() -> Unit) { t(call).f() }

    override fun callStart(call: Call) { delegate?.callStart(call); timings[call] = CallTiming() }
    override fun dispatcherQueueStart(call: Call, dispatcher: okhttp3.Dispatcher) { delegate?.dispatcherQueueStart(call, dispatcher) }
    override fun dispatcherQueueEnd(call: Call, dispatcher: okhttp3.Dispatcher) { delegate?.dispatcherQueueEnd(call, dispatcher) }
    override fun proxySelectStart(call: Call, url: HttpUrl) { delegate?.proxySelectStart(call, url) }
    override fun proxySelectEnd(call: Call, url: HttpUrl, proxies: List<Proxy>) { delegate?.proxySelectEnd(call, url, proxies) }
    override fun dnsStart(call: Call, domainName: String) { delegate?.dnsStart(call, domainName); mine(call) { dnsStart = System.nanoTime() } }
    override fun dnsEnd(call: Call, domainName: String, inetAddressList: List<InetAddress>) { delegate?.dnsEnd(call, domainName, inetAddressList); mine(call) { dnsEnd = System.nanoTime() } }
    override fun connectStart(call: Call, inetSocketAddress: InetSocketAddress, proxy: Proxy) { delegate?.connectStart(call, inetSocketAddress, proxy); mine(call) { connectStart = System.nanoTime() } }
    override fun secureConnectStart(call: Call) { delegate?.secureConnectStart(call); mine(call) { tlsStart = System.nanoTime() } }
    override fun secureConnectEnd(call: Call, handshake: Handshake?) { delegate?.secureConnectEnd(call, handshake); mine(call) { tlsEnd = System.nanoTime() } }
    override fun connectEnd(call: Call, inetSocketAddress: InetSocketAddress, proxy: Proxy, protocol: Protocol?) { delegate?.connectEnd(call, inetSocketAddress, proxy, protocol); mine(call) { connectEnd = System.nanoTime() } }
    override fun connectFailed(call: Call, inetSocketAddress: InetSocketAddress, proxy: Proxy, protocol: Protocol?, ioe: IOException) { delegate?.connectFailed(call, inetSocketAddress, proxy, protocol, ioe); mine(call) { connectFailures++ } }
    override fun connectionAcquired(call: Call, connection: Connection) { delegate?.connectionAcquired(call, connection) }
    override fun connectionReleased(call: Call, connection: Connection) { delegate?.connectionReleased(call, connection) }
    override fun requestHeadersStart(call: Call) { delegate?.requestHeadersStart(call); mine(call) { if (reqStart == 0L) reqStart = System.nanoTime() } }
    override fun requestHeadersEnd(call: Call, request: Request) { delegate?.requestHeadersEnd(call, request); mine(call) { reqEnd = System.nanoTime() } }
    override fun requestBodyStart(call: Call) { delegate?.requestBodyStart(call) }
    override fun requestBodyEnd(call: Call, byteCount: Long) { delegate?.requestBodyEnd(call, byteCount); mine(call) { reqEnd = System.nanoTime() } }
    override fun requestFailed(call: Call, ioe: IOException) { delegate?.requestFailed(call, ioe) }
    override fun responseHeadersStart(call: Call) { delegate?.responseHeadersStart(call); mine(call) { resHeadersStart = System.nanoTime() } }
    override fun responseHeadersEnd(call: Call, response: Response) { delegate?.responseHeadersEnd(call, response); mine(call) { resHeadersEnd = System.nanoTime() } }
    override fun responseBodyStart(call: Call) { delegate?.responseBodyStart(call) }
    override fun responseBodyEnd(call: Call, byteCount: Long) { delegate?.responseBodyEnd(call, byteCount); mine(call) { resBodyEnd = System.nanoTime() } }
    override fun responseFailed(call: Call, ioe: IOException) { delegate?.responseFailed(call, ioe) }
    override fun callEnd(call: Call) { delegate?.callEnd(call); timings[call]?.finish?.invoke() }
    override fun callFailed(call: Call, ioe: IOException) { delegate?.callFailed(call, ioe); timings[call]?.finish?.invoke() }
    override fun canceled(call: Call) { delegate?.canceled(call) }
    override fun satisfactionFailure(call: Call, response: Response) { delegate?.satisfactionFailure(call, response) }
    override fun cacheHit(call: Call, response: Response) { delegate?.cacheHit(call, response); mine(call) { cacheHit = true } }
    override fun cacheMiss(call: Call) { delegate?.cacheMiss(call) }
    override fun cacheConditionalHit(call: Call, cachedResponse: Response) { delegate?.cacheConditionalHit(call, cachedResponse) }
    override fun retryDecision(call: Call, exception: IOException, retry: Boolean) { delegate?.retryDecision(call, exception, retry) }
    override fun followUpDecision(call: Call, networkResponse: Response, nextRequest: Request?) { delegate?.followUpDecision(call, networkResponse, nextRequest) }

    internal class Factory(private val delegate: EventListener.Factory?) : EventListener.Factory {
        override fun create(call: Call): EventListener = InspectorEventListener(delegate?.create(call))
        val wrapped: EventListener.Factory? get() = delegate
    }

    companion object {
        /** Keyed weakly, so a call that never ends cannot leak. */
        internal val timings: MutableMap<Call, CallTiming> = Collections.synchronizedMap(WeakHashMap())
    }
}
