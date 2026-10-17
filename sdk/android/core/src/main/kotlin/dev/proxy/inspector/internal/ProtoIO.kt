package dev.proxy.inspector.internal

import java.io.ByteArrayOutputStream

/**
 * Just enough protobuf for `proto/sdk.proto`: varints and length-delimited fields. Hand-rolled so
 * the SDK does not pull a second protobuf runtime into the host app. proto3 rules: zero and empty
 * values are not written, so the bytes match what the Rust side (prost) produces.
 */
class ProtoWriter {
    private val out = ByteArrayOutputStream(256)

    fun toByteArray(): ByteArray = out.toByteArray()

    private fun rawVarint(value: Long) {
        var v = value
        while (v and 0x7FL.inv() != 0L) {
            out.write(((v and 0x7F) or 0x80).toInt())
            v = v ushr 7
        }
        out.write(v.toInt())
    }

    private fun tag(field: Int, wire: Int) = rawVarint(((field shl 3) or wire).toLong())

    /** int32, int64, uint32, uint64, enum. Negative int32 values use the 10-byte form, as protobuf specifies. */
    fun int(field: Int, v: Long) {
        if (v == 0L) return
        tag(field, 0); rawVarint(v)
    }

    fun int(field: Int, v: Int) = int(field, v.toLong())

    fun bool(field: Int, v: Boolean) {
        if (!v) return
        tag(field, 0); rawVarint(1)
    }

    fun bytes(field: Int, v: ByteArray) {
        if (v.isEmpty()) return
        tag(field, 2); rawVarint(v.size.toLong()); out.write(v)
    }

    fun string(field: Int, v: String) {
        if (v.isEmpty()) return
        bytes(field, v.toByteArray(Charsets.UTF_8))
    }

    /** A nested message. Written even when empty, since presence carries meaning. */
    fun message(field: Int, v: ByteArray) {
        tag(field, 2); rawVarint(v.size.toLong()); out.write(v)
    }
}

class ProtoReader(private val buf: ByteArray, private var pos: Int = 0, private val end: Int = buf.size) {
    var field = 0
        private set
    var wire = 0
        private set

    /** Advance to the next field. False at the end of the message. */
    fun next(): Boolean {
        if (pos >= end) return false
        val t = varint().toInt()
        field = t ushr 3
        wire = t and 7
        if (field == 0) throw ProtoException("field number 0")
        return true
    }

    fun varint(): Long {
        var shift = 0
        var result = 0L
        while (true) {
            if (pos >= end || shift > 63) throw ProtoException("truncated varint")
            val b = buf[pos++].toInt()
            result = result or ((b and 0x7F).toLong() shl shift)
            if (b and 0x80 == 0) return result
            shift += 7
        }
    }

    fun int(): Int = varint().toInt()
    fun bool(): Boolean = varint() != 0L

    fun bytes(): ByteArray {
        val n = varint()
        if (n < 0 || n > end - pos) throw ProtoException("length $n runs past the message")
        val r = buf.copyOfRange(pos, pos + n.toInt())
        pos += n.toInt()
        return r
    }

    fun string(): String = String(bytes(), Charsets.UTF_8)

    /** Skip the value of an unknown field, so newer peers stay readable. */
    fun skip() {
        when (wire) {
            0 -> varint()
            1 -> advance(8)
            2 -> { val n = varint(); if (n < 0 || n > end - pos) throw ProtoException("bad length"); advance(n.toInt()) }
            5 -> advance(4)
            else -> throw ProtoException("unsupported wire type $wire")
        }
    }

    private fun advance(n: Int) {
        if (pos + n > end) throw ProtoException("truncated")
        pos += n
    }
}

class ProtoException(message: String) : RuntimeException(message)
