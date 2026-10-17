package dev.proxy.inspector.internal

/**
 * Hides secrets before anything leaves the device. Runs on the sender thread.
 *
 * DEFAULT replaces: Authorization, Cookie, Set-Cookie, Proxy-Authorization and API-key-like
 * headers; JSON and form body values under sensitive keys; sensitive query parameters.
 * STRICT replaces every header outside a short safe list and sends no bodies. NONE replaces nothing.
 */
class Redactor private constructor(private val level: Int, private val keys: Set<String>) {
    constructor(level: Int, extraKeys: List<String>) : this(level, (BASE_KEYS + extraKeys.map { it.lowercase().replace("-", "_") }).toSet())

    /** A fresh redactor with the same policy, so each call tracks its own `changed`. */
    fun fork() = Redactor(level, keys)

    /** True if any value was replaced since the last [reset]. */
    var changed = false
        private set

    fun reset() { changed = false }

    val bodiesAllowed: Boolean get() = level != RedactionLevel.STRICT

    fun header(name: String, value: String): String {
        if (level == RedactionLevel.NONE) return value
        val n = name.lowercase()
        val hide = if (level == RedactionLevel.STRICT) n !in SAFE_HEADERS else isSensitiveHeader(n)
        if (!hide) return value
        changed = true
        return MARK
    }

    fun headers(list: List<Pair<String, String>>): List<Header> = list.map { (n, v) -> Header(n, header(n, v)) }

    /** Query parameter values under sensitive keys. */
    fun url(url: String): String {
        if (level == RedactionLevel.NONE) return url
        val q = url.indexOf('?')
        if (q < 0) return url
        val frag = url.indexOf('#', q).let { if (it < 0) url.length else it }
        val query = url.substring(q + 1, frag)
        val out = redactPairs(query)
        return if (out === query) url else url.substring(0, q + 1) + out + url.substring(frag)
    }

    /** A body, given its content type. Anything that is not JSON or form data passes through. */
    fun body(contentType: String, bytes: ByteArray): ByteArray {
        if (level == RedactionLevel.NONE || bytes.isEmpty()) return bytes
        var ct = contentType.lowercase()
        // No content type at all: a body that looks like JSON is treated as JSON, never left in the clear.
        if (ct.isBlank() && bytes[0].toInt().toChar().let { it == '{' || it == '[' }) ct = "application/json"
        return when {
            "json" in ct -> {
                val s = utf8OrNull(bytes) ?: return bytes
                val r = redactJson(s)
                if (r === s) bytes else r.toByteArray(Charsets.UTF_8)
            }
            "x-www-form-urlencoded" in ct -> {
                val s = utf8OrNull(bytes) ?: return bytes
                val r = redactPairs(s)
                if (r === s) bytes else r.toByteArray(Charsets.UTF_8)
            }
            else -> bytes
        }
    }

    private fun utf8OrNull(b: ByteArray): String? = try {
        Charsets.UTF_8.newDecoder().decode(java.nio.ByteBuffer.wrap(b)).toString()
    } catch (e: java.nio.charset.CharacterCodingException) { null }

    private fun isSensitiveHeader(n: String): Boolean =
        n in SENSITIVE_HEADERS || n.contains("api-key") || n.contains("apikey") || n.contains("api_key") ||
            n.endsWith("-token") || n.contains("secret") || n.contains("auth")

    /** `a=1&password=x` -> `a=1&password=[redacted]`. Returns the same instance when nothing changed. */
    private fun redactPairs(s: String): String {
        if (s.isEmpty()) return s
        var hit = false
        val parts = s.split('&').map { p ->
            val eq = p.indexOf('=')
            if (eq > 0 && isSensitiveKey(decode(p.substring(0, eq))) && eq < p.length - 1) {
                hit = true; p.substring(0, eq + 1) + MARK
            } else p
        }
        if (!hit) return s
        changed = true
        return parts.joinToString("&")
    }

    private fun decode(s: String) = try { java.net.URLDecoder.decode(s, "UTF-8") } catch (e: Exception) { s }

    private fun isSensitiveKey(k: String): Boolean = k.lowercase().replace("-", "_") in keys

    /**
     * Replaces the value of every sensitive key. A small scanner rather than a parser: it never
     * fails on malformed JSON, it just stops redacting where the text stops making sense.
     */
    internal fun redactJson(s: String): String {
        var out: StringBuilder? = null
        var copied = 0
        var i = 0
        val n = s.length
        while (i < n) {
            if (s[i] != '"') { i++; continue }
            val keyEnd = stringEnd(s, i)
            if (keyEnd < 0) break
            var j = keyEnd + 1
            while (j < n && s[j].isWhitespace()) j++
            if (j < n && s[j] == ':' && isSensitiveKey(unescape(s.substring(i + 1, keyEnd)))) {
                var v = j + 1
                while (v < n && s[v].isWhitespace()) v++
                val vEnd = valueEnd(s, v)
                if (vEnd > v) {
                    val sb = out ?: StringBuilder(s.length).also { out = it }
                    sb.append(s, copied, v).append('"').append(MARK).append('"')
                    copied = vEnd
                    i = vEnd
                    changed = true
                    continue
                }
            }
            i = keyEnd + 1
        }
        val sb = out ?: return s
        sb.append(s, copied, n)
        return sb.toString()
    }

    private fun unescape(s: String) = if ('\\' in s) s.replace("\\\"", "\"").replace("\\\\", "\\") else s

    /** Index of the closing quote of the string starting at [start], or -1. */
    private fun stringEnd(s: String, start: Int): Int {
        var i = start + 1
        while (i < s.length) {
            when (s[i]) { '\\' -> i++; '"' -> return i }
            i++
        }
        return -1
    }

    /** End (exclusive) of the JSON value starting at [start]. */
    private fun valueEnd(s: String, start: Int): Int {
        if (start >= s.length) return start
        return when (s[start]) {
            '"' -> stringEnd(s, start).let { if (it < 0) start else it + 1 }
            '{', '[' -> {
                var depth = 0
                var i = start
                while (i < s.length) {
                    when (s[i]) {
                        '"' -> { val e = stringEnd(s, i); if (e < 0) return start; i = e }
                        '{', '[' -> depth++
                        '}', ']' -> { depth--; if (depth == 0) return i + 1 }
                    }
                    i++
                }
                start
            }
            else -> {
                var i = start
                while (i < s.length && s[i] !in ",}] \n\r\t") i++
                i
            }
        }
    }

    companion object {
        const val MARK = "[redacted]"

        private val SENSITIVE_HEADERS = setOf(
            "authorization", "proxy-authorization", "cookie", "set-cookie", "x-csrf-token", "x-xsrf-token",
        )

        private val SAFE_HEADERS = setOf(
            "accept", "accept-encoding", "accept-language", "cache-control", "content-encoding", "content-length",
            "content-type", "date", "etag", "host", "if-none-match", "last-modified", "server", "user-agent", "vary",
            "connection", "transfer-encoding", "age", "expires",
        )

        private val BASE_KEYS = setOf(
            "password", "passwd", "pwd", "token", "access_token", "refresh_token", "id_token", "secret",
            "client_secret", "api_key", "apikey", "authorization", "otp", "pin", "cvv", "cvc", "card_number",
            "cardnumber", "ssn", "session_token", "auth_token",
        )
    }
}
