package dev.proxy.inspector.internal

import java.io.DataInputStream
import java.io.EOFException
import java.io.InputStream
import java.io.OutputStream
import java.security.MessageDigest
import java.security.SecureRandom

const val MAX_FRAME = 1 shl 20

/** Length-prefixed frames: 4 bytes big-endian, then one [Envelope]. */
object Frames {
    fun write(out: OutputStream, env: Envelope) {
        val body = env.encode()
        require(body.size <= MAX_FRAME) { "frame of ${body.size} bytes" }
        val n = body.size
        val buf = ByteArray(4 + n)
        buf[0] = (n ushr 24).toByte(); buf[1] = (n ushr 16).toByte(); buf[2] = (n ushr 8).toByte(); buf[3] = n.toByte()
        System.arraycopy(body, 0, buf, 4, n)
        out.write(buf)
    }

    /** Null at a clean end of stream between frames. */
    fun read(input: InputStream): Envelope? {
        val data = DataInputStream(input)
        val n = try { data.readInt() } catch (e: EOFException) { return null }
        if (n <= 0 || n > MAX_FRAME) throw ProtoException("bad frame length $n")
        val body = ByteArray(n)
        data.readFully(body)
        return Envelope.decode(body)
    }
}

/** Commit-reveal pairing math. Must agree byte for byte with the Rust side (`sdk::pairing`). */
object Pairing {
    private val random = SecureRandom()

    fun nonce(): ByteArray = ByteArray(32).also { random.nextBytes(it) }

    fun commit(clientNonce: ByteArray): ByteArray = MessageDigest.getInstance("SHA-256").digest(clientNonce)

    /** The six digits both screens show. */
    fun code(fingerprint: ByteArray, clientNonce: ByteArray, serverNonce: ByteArray): String {
        val d = MessageDigest.getInstance("SHA-256").run {
            update(fingerprint); update(clientNonce); update(serverNonce); digest()
        }
        val n = ((d[0].toLong() and 0xFF) shl 24) or ((d[1].toLong() and 0xFF) shl 16) or
            ((d[2].toLong() and 0xFF) shl 8) or (d[3].toLong() and 0xFF)
        return (n % 1_000_000).toString().padStart(6, '0')
    }

    fun spaced(code: String) = if (code.length == 6) "${code.substring(0, 3)} ${code.substring(3)}" else code
}

fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }

fun String.hexToBytes(): ByteArray =
    ByteArray(length / 2) { ((this[it * 2].digitToInt(16) shl 4) or this[it * 2 + 1].digitToInt(16)).toByte() }
