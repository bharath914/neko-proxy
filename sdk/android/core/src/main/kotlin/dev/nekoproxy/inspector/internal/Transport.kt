package dev.nekoproxy.inspector.internal

import java.net.InetSocketAddress
import java.net.Socket
import java.security.MessageDigest
import java.security.SecureRandom
import java.security.cert.X509Certificate
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLSocket
import javax.net.ssl.X509TrustManager

/** The desktop presented a different certificate than the one this app paired with. */
class FingerprintMismatch(val expected: ByteArray, val saw: ByteArray) :
    RuntimeException("certificate fingerprint ${saw.toHex().take(16)} does not match the paired desktop (${expected.toHex().take(16)})")

class Connected(val socket: SSLSocket, val fingerprint: ByteArray)

/**
 * TLS over a plain [SSLSocket]. The desktop's certificate is self-signed, so nothing is trusted
 * through a CA: while paired the SHA-256 of the DER certificate must equal the stored one; while
 * pairing any certificate is accepted for the handshake and the fingerprint seen is returned, so
 * the pairing code can be computed from it.
 */
object Transport {
    fun connect(host: String, port: Int, pinned: ByteArray?, connectTimeoutMs: Int, readTimeoutMs: Int = 0): Connected {
        var seen: ByteArray? = null
        val trust = object : X509TrustManager {
            override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) = throw UnsupportedOperationException()
            override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {
                val leaf = chain?.firstOrNull() ?: throw java.security.cert.CertificateException("no certificate")
                val fp = MessageDigest.getInstance("SHA-256").digest(leaf.encoded)
                seen = fp
                if (pinned != null && !MessageDigest.isEqual(pinned, fp)) throw FingerprintMismatch(pinned, fp)
            }
            override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
        }
        val ctx = SSLContext.getInstance("TLS").apply { init(null, arrayOf(trust), SecureRandom()) }
        val raw = Socket()
        try {
            raw.tcpNoDelay = true
            raw.connect(InetSocketAddress(host, port), connectTimeoutMs)
            raw.soTimeout = if (readTimeoutMs > 0) readTimeoutMs else connectTimeoutMs
            // No SNI or hostname check: the connection is by address, and the pin replaces them.
            val socket = ctx.socketFactory.createSocket(raw, host, port, true) as SSLSocket
            socket.startHandshake()
            socket.soTimeout = 0
            return Connected(socket, seen ?: throw java.io.IOException("no certificate seen"))
        } catch (e: Exception) {
            runCatching { raw.close() }
            // The trust manager's exception is wrapped by the TLS stack; surface the mismatch itself.
            var c: Throwable? = e
            while (c != null) { if (c is FingerprintMismatch) throw c; c = c.cause }
            throw e
        }
    }
}
