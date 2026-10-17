package dev.proxy.inspector.internal

import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.util.concurrent.ConcurrentLinkedQueue

/** Receives frames on the sender thread. */
fun interface Sink { fun send(env: Envelope) }

/** The settings capture runs under, once a desktop has accepted this app. */
class Live(val config: Config, val engine: Engine) {
    val redactor = Redactor(config.redaction, config.extraRedactKeys)

    fun hostAllowed(host: String): Boolean {
        val h = host.lowercase()
        if (config.hostDeny.any { match(it, h) }) return false
        return config.hostAllow.isEmpty() || config.hostAllow.any { match(it, h) }
    }

    private fun match(pattern: String, host: String): Boolean {
        val p = pattern.lowercase()
        return if (p.startsWith("*.")) host.endsWith(p.substring(1)) || host == p.substring(2) else host == p
    }
}

enum class State { STOPPED, DISCONNECTED, CONNECTING, PAIRING, CONNECTED }

/**
 * Owns the connection to the desktop: discovery, TLS, pairing, and the single sender thread.
 * Capture is on only while this holds a [Live]; otherwise requests pay one volatile read.
 */
class Engine(
    val platform: Platform,
    private val settings: () -> EngineSettings,
) {
    data class EngineSettings(val host: String? = null, val port: Int = DEFAULT_PORT, val desktopName: String? = null)

    private val store = PairingStore(platform.storageDir)
    val ring = BodyRing()
    private val queue = DropOldestQueue<(Sink) -> Unit>(2048)
    private val control = ConcurrentLinkedQueue<(Sink) -> Unit>()

    @Volatile var live: Live? = null
        private set
    @Volatile var state: State = State.STOPPED
        private set
    @Volatile var screen: String = ""

    @Volatile private var stopped = true
    private var thread: Thread? = null
    private var lastCertWarning = 0L

    fun dropped(): Long = queue.dropped()

    /** Bytes written to the desktop since start, for the benchmarks. */
    val bytesSent = java.util.concurrent.atomic.AtomicLong()

    @Synchronized
    fun start() {
        if (!stopped) return
        stopped = false
        state = State.DISCONNECTED
        thread = Thread(::run, "inspector-io").apply { isDaemon = true; start() }
    }

    @Synchronized
    fun stop() {
        stopped = true
        thread?.interrupt()
        thread = null
        live = null
        state = State.STOPPED
    }

    /** Forget the pairing; the next connection pairs again. */
    fun forgetPairing() = store.clear()

    /** Queue work for the sender thread. Only call while capture is active. */
    fun emit(job: (Sink) -> Unit) = queue.offer(job)

    private fun log(level: LogLevel, msg: String) = platform.log(level, msg)

    // ------------------------------------------------------------ connection loop

    private fun run() {
        val backoff = longArrayOf(2_000, 5_000, 15_000, 60_000)
        var failures = 0
        while (!stopped) {
            val retryMs: Long = try {
                attempt().also { if (it == RETRY_RESET) failures = 0 }
            } catch (e: InterruptedException) {
                return
            } catch (e: Exception) {
                log(LogLevel.DEBUG, "connection attempt failed: ${e.message}")
                RETRY_BACKOFF
            }
            live = null
            if (state != State.STOPPED) state = State.DISCONNECTED
            val wait = when (retryMs) {
                RETRY_RESET -> backoff[0]
                RETRY_BACKOFF -> backoff[minOf(failures++, backoff.size - 1)]
                else -> retryMs
            }
            try { Thread.sleep(wait) } catch (e: InterruptedException) { return }
        }
    }

    private fun candidates(pairing: PairingStore.Pairing?, s: EngineSettings): List<Candidate> {
        val list = ArrayList<Candidate>()
        s.host?.let { list += Candidate(it, s.port, "wifi", desktopName = s.desktopName) }
        if (pairing != null && pairing.lastHost.isNotEmpty() && pairing.lastPort > 0) {
            list += Candidate(pairing.lastHost, pairing.lastPort, "")
        }
        list += platform.localCandidates().map { it.copy(port = if (it.port == 0) s.port else it.port) }
        return list.distinctBy { it.host to it.port }
    }

    /** Returns how long to wait before the next attempt. */
    private fun attempt(): Long {
        val s = settings()
        val pairing = store.load()
        state = State.CONNECTING
        var mismatch: FingerprintMismatch? = null
        var refused = 0L
        for (c in candidates(pairing, s)) {
            if (stopped) return 0
            val conn = try {
                Transport.connect(c.host, c.port, pairing?.fingerprint, connectTimeoutMs = 1_500)
            } catch (e: FingerprintMismatch) {
                mismatch = e; continue
            } catch (e: Exception) { continue }
            return session(conn, c, pairing, s)
        }
        // Nothing local answered: look on the LAN, briefly, only while disconnected.
        val found = platform.discover(2_000)
        val usable = found.filter { pairing == null || it.fingerprintShort == null || pairing.fingerprint.toHex().startsWith(it.fingerprintShort) }
        if (pairing == null) {
            val distinct = found.mapNotNull { it.fingerprintShort ?: it.desktopName }.distinct()
            val wanted = s.desktopName
            val pick = when {
                wanted != null -> found.filter { it.desktopName.equals(wanted, true) }
                distinct.size > 1 -> {
                    log(LogLevel.WARN, "Several desktops found: ${found.mapNotNull { it.desktopName }.distinct().joinToString()}. " +
                        "Not pairing automatically. Choose one with Inspector.desktop(\"name\").")
                    emptyList()
                }
                else -> found
            }
            for (c in pick) {
                val conn = try { Transport.connect(c.host, c.port, null, 2_000) } catch (e: Exception) { continue }
                return session(conn, c, null, s)
            }
        } else {
            for (c in usable) {
                val conn = try { Transport.connect(c.host, c.port, pairing.fingerprint, 2_000) } catch (e: FingerprintMismatch) { mismatch = e; continue } catch (e: Exception) { continue }
                return session(conn, c, pairing, s)
            }
        }
        if (mismatch != null && System.currentTimeMillis() - lastCertWarning > 60_000) {
            lastCertWarning = System.currentTimeMillis()
            log(
                LogLevel.ERROR,
                "A desktop answered with a different certificate (${mismatch.saw.toHex().take(16)}) than the one this app paired with. " +
                    "Not connecting. If you replaced the certificate on the desktop, call Inspector.forgetPairing() and run again.",
            )
        }
        return if (refused > 0) refused else RETRY_BACKOFF
    }

    // ------------------------------------------------------------ one connection

    private fun session(conn: Connected, c: Candidate, pairing: PairingStore.Pairing?, s: EngineSettings): Long {
        val socket = conn.socket
        try {
            val input = BufferedInputStream(socket.getInputStream(), 16 * 1024)
            val out = BufferedOutputStream(socket.getOutputStream(), 16 * 1024)
            socket.soTimeout = 10_000
            val kind = c.kind.ifEmpty { if (c.host == "localhost" || c.host == "127.0.0.1") "usb" else "wifi" }
            Frames.write(out, Envelope(Envelope.HELLO, Hello(
                sdkName = platform.sdkName, sdkVersion = SDK_VERSION, appId = platform.appId, appName = platform.appName,
                appVersion = platform.appVersion, build = platform.build, deviceInstallId = store.installId(),
                deviceName = platform.deviceName, deviceModel = platform.deviceModel, osVersion = platform.osVersion,
                deviceTimeMs = System.currentTimeMillis(), token = pairing?.token ?: ByteArray(0), connection = kind,
            ).encode()))
            out.flush()
            val first = Frames.read(input) ?: return RETRY_BACKOFF
            if (first.kind == Envelope.ERROR) { logWireError(WireError.decode(first.body)); return 60_000 }
            if (first.kind != Envelope.HELLO_ACK) return RETRY_BACKOFF
            val ack = HelloAck.decode(first.body)
            if (s.desktopName != null && ack.desktopName.isNotEmpty() && !ack.desktopName.equals(s.desktopName, true) && pairing == null) {
                log(LogLevel.INFO, "Skipping desktop \"${ack.desktopName}\" (looking for \"${s.desktopName}\").")
                return RETRY_BACKOFF
            }
            var paired = pairing
            when (ack.state) {
                AckState.AUTHENTICATED -> {}
                AckState.PAIRING_REQUIRED -> {
                    state = State.PAIRING
                    paired = pair(input, out, conn, c, ack) ?: return 600_000
                }
                AckState.DENIED -> {
                    log(LogLevel.WARN, "The desktop denied this app. ${ack.message} Backing off.")
                    return maxOf(ack.retryAfterS * 1000L, 60_000)
                }
                else -> {
                    log(LogLevel.DEBUG, "Desktop is not accepting SDK connections: ${ack.message}")
                    return maxOf(ack.retryAfterS * 1000L, 5_000)
                }
            }
            // Remember where the desktop was, so the next start tries it first.
            if (paired != null) store.save(paired.copy(lastHost = c.host, lastPort = c.port, desktopName = ack.desktopName))
            return authenticated(socket, input, out, ack.desktopName, c)
        } finally {
            runCatching { socket.close() }
        }
    }

    private fun pair(input: BufferedInputStream, out: BufferedOutputStream, conn: Connected, c: Candidate, ack: HelloAck): PairingStore.Pairing? {
        val nonce = Pairing.nonce()
        Frames.write(out, Envelope(Envelope.PAIR_REQUEST, PairRequest(Pairing.commit(nonce)).encode())); out.flush()
        val challenge = expect(input, out, Envelope.PAIR_CHALLENGE)?.let { PairChallenge.decode(it.body) } ?: return null
        Frames.write(out, Envelope(Envelope.PAIR_REVEAL, PairReveal(nonce).encode())); out.flush()
        val code = Pairing.spaced(Pairing.code(conn.fingerprint, nonce, challenge.serverNonce))
        val name = ack.desktopName.ifEmpty { c.host }
        log(LogLevel.WARN, "Pairing code $code for $name. Click Allow on the desktop if it shows the same code.")
        platform.notify("Inspector pairing code $code. Check it on $name.")
        conn.socket.soTimeout = (challenge.timeoutS.coerceAtLeast(10) + 10) * 1000
        val result = expect(input, out, Envelope.PAIR_RESULT)?.let { PairResult.decode(it.body) } ?: return null
        if (!result.allowed) {
            log(LogLevel.WARN, "Pairing was not accepted: ${result.message}")
            return null
        }
        log(LogLevel.INFO, "Paired with $name.")
        conn.socket.soTimeout = 10_000
        return PairingStore.Pairing(result.token, conn.fingerprint, c.host, c.port, ack.desktopName)
            .also { store.save(it) }
    }

    /** Next message of [kind], answering pings meanwhile. Null on an error or the wrong message. */
    private fun expect(input: BufferedInputStream, out: BufferedOutputStream, kind: Int): Envelope? {
        while (true) {
            val e = Frames.read(input) ?: return null
            when (e.kind) {
                kind -> return e
                Envelope.PING -> { Frames.write(out, Envelope(Envelope.PONG, Pong(Ping.decode(e.body).timeMs, System.currentTimeMillis()).encode())); out.flush() }
                Envelope.ERROR -> { logWireError(WireError.decode(e.body)); return null }
                else -> return null
            }
        }
    }

    private fun logWireError(e: WireError) {
        if (e.code == ErrorCode.UNSUPPORTED_VERSION) {
            log(LogLevel.ERROR, "The desktop does not support this SDK's protocol version ($PROTOCOL_VERSION): ${e.message} " +
                "It speaks ${e.minVersion} to ${e.maxVersion}. Update the desktop app or the SDK.")
        } else log(LogLevel.WARN, "The desktop closed the connection: ${e.message}")
    }

    private fun authenticated(socket: javax.net.ssl.SSLSocket, input: BufferedInputStream, out: BufferedOutputStream, desktop: String, c: Candidate): Long {
        socket.soTimeout = 0
        val readerDead = java.util.concurrent.atomic.AtomicBoolean(false)
        var lastRead = System.currentTimeMillis()
        val readerLock = Any()
        val reader = Thread({
            try {
                while (true) {
                    val e = Frames.read(input) ?: break
                    synchronized(readerLock) { lastRead = System.currentTimeMillis() }
                    when (e.kind) {
                        Envelope.CONFIG -> {
                            val cfg = Config.decode(e.body)
                            live = if (cfg.captureEnabled) Live(cfg, this) else null
                            state = State.CONNECTED
                        }
                        Envelope.PING -> {
                            val p = Ping.decode(e.body)
                            control.add { it.send(Envelope(Envelope.PONG, Pong(p.timeMs, System.currentTimeMillis()).encode())) }
                        }
                        Envelope.BODY_FETCH -> {
                            val f = BodyFetch.decode(e.body)
                            control.add { sink -> answerFetch(sink, f) }
                        }
                        Envelope.ERROR -> { logWireError(WireError.decode(e.body)); break }
                    }
                }
            } catch (e: Exception) {
                // closed or malformed: the main loop notices
            } finally { readerDead.set(true) }
        }, "inspector-read").apply { isDaemon = true; start() }

        log(LogLevel.INFO, "Connected to $desktop (${c.host}:${c.port}).")
        val sink = Sink { bytesSent.addAndGet(Frames.write(out, it).toLong()) }
        var lastWrite = System.currentTimeMillis()
        try {
            while (!stopped && !readerDead.get()) {
                var wrote = false
                while (true) { val j = control.poll() ?: break; j(sink); wrote = true }
                val job = queue.poll(500)
                if (job != null) { job(sink); wrote = true }
                if (wrote) lastWrite = System.currentTimeMillis()
                if (wrote && queue.size() == 0 && control.isEmpty()) out.flush()
                val now = System.currentTimeMillis()
                if (now - lastWrite >= 15_000) {
                    Frames.write(out, Envelope(Envelope.PING, Ping(now).encode())); out.flush(); lastWrite = now
                }
                if (now - synchronized(readerLock) { lastRead } > 60_000) { log(LogLevel.DEBUG, "desktop went silent"); break }
            }
        } catch (e: Exception) {
            log(LogLevel.DEBUG, "connection lost: ${e.message}")
        } finally {
            live = null
            queue.clear(); control.clear()
            runCatching { socket.close() }
            reader.interrupt()
            log(LogLevel.INFO, "Disconnected from $desktop.")
        }
        return RETRY_RESET
    }

    /** The desktop opened a flow whose body was too big to send eagerly. */
    private fun answerFetch(sink: Sink, f: BodyFetch) {
        val body = ring.get(f.callId, f.part)
        sink.send(Envelope(Envelope.BODY_FETCH_RESULT, BodyFetchResult(f.callId, f.part, body != null).encode()))
        if (body != null) Bodies.sendChunks(sink, f.callId, f.part, body, 0, last = true, truncated = false)
    }

    companion object {
        const val DEFAULT_PORT = 8897
        const val SDK_VERSION = "0.1.0"
        private const val RETRY_RESET = -1L
        private const val RETRY_BACKOFF = -2L
    }
}

/** Chunking and compression of bodies for the wire. */
object Bodies {
    const val CHUNK = 64 * 1024

    fun sendChunks(sink: Sink, callId: String, part: Int, body: ByteArray, firstSeq: Int, last: Boolean, truncated: Boolean) {
        var seq = firstSeq
        var off = 0
        if (body.isEmpty()) {
            if (last || truncated) sink.send(Envelope(Envelope.BODY_CHUNK, BodyChunk(callId, part, seq, ByteArray(0), last = last, truncated = truncated).encode()))
            return
        }
        while (off < body.size) {
            val n = minOf(CHUNK, body.size - off)
            val isLast = off + n >= body.size
            val raw = body.copyOfRange(off, off + n)
            val (data, enc) = deflateIfSmaller(raw)
            sink.send(Envelope(Envelope.BODY_CHUNK, BodyChunk(callId, part, seq++, data, enc, last = last && isLast, truncated = truncated && isLast).encode()))
            off += n
        }
    }

    /** One deflater per sender thread: creating one per chunk cost more than the compression saved on small bodies. */
    private val deflater = ThreadLocal.withInitial { java.util.zip.Deflater(java.util.zip.Deflater.BEST_SPEED, true) }

    /** Raw deflate (no zlib header), kept only when it actually shrinks the chunk. Chunks under 512 bytes go as they are. */
    fun deflateIfSmaller(raw: ByteArray): Pair<ByteArray, Int> {
        if (raw.size < 512) return raw to Encoding.NONE
        val d = deflater.get()
        d.reset()
        d.setInput(raw); d.finish()
        val buf = ByteArray(raw.size)
        val n = d.deflate(buf)
        if (!d.finished() || n >= raw.size) return raw to Encoding.NONE
        return buf.copyOf(n) to Encoding.DEFLATE
    }
}
