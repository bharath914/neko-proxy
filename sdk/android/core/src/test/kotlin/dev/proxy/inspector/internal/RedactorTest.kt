package dev.proxy.inspector.internal

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class RedactorTest {
    private val r = Redactor(RedactionLevel.DEFAULT, listOf("national_id"))

    @Test fun sensitiveHeadersAreHidden() {
        for (h in listOf("Authorization", "Cookie", "Set-Cookie", "Proxy-Authorization", "X-Api-Key", "X-Auth-Token", "apikey", "X-Client-Secret"))
            assertEquals("[redacted]", r.fork().header(h, "v"), h)
        assertEquals("application/json", r.fork().header("Content-Type", "application/json"))
        assertEquals("okhttp", r.fork().header("User-Agent", "okhttp"))
    }

    @Test fun jsonValuesUnderSensitiveKeysAreReplacedWhateverTheirType() {
        val x = r.fork()
        val out = x.redactJson("""{"user":"a","password":"p w","token":123,"nested":{"refresh_token":"t","ok":true},"card_number":["4111"],"national_id":{"a":1},"keep":null}""")
        assertEquals("""{"user":"a","password":"[redacted]","token":"[redacted]","nested":{"refresh_token":"[redacted]","ok":true},"card_number":"[redacted]","national_id":"[redacted]","keep":null}""", out)
        assertTrue(x.changed)
    }

    @Test fun jsonScannerSurvivesGarbageAndEscapes() {
        val x = r.fork()
        assertEquals("not json at all", x.redactJson("not json at all"))
        assertEquals("""{"a":"unterminated""", x.redactJson("""{"a":"unterminated"""))
        assertEquals("""{"pass\"word":"x","password":"[redacted]"}""", x.redactJson("""{"pass\"word":"x","password":"secret \" quote"}"""))
        assertFalse(r.fork().also { it.redactJson("""{"a":1}""") }.changed)
    }

    @Test fun formBodiesAndUrlsAreRedacted() {
        val x = r.fork()
        assertEquals("a=1&password=[redacted]&b=2", String(x.body("application/x-www-form-urlencoded", "a=1&password=hunter2&b=2".toByteArray())))
        assertEquals("https://x.test/p?q=1&access_token=[redacted]#f", x.url("https://x.test/p?q=1&access_token=abc#f"))
        assertEquals("https://x.test/p?q=1", r.fork().url("https://x.test/p?q=1"))
    }

    @Test fun binaryAndOtherTypesPassThroughUntouched() {
        val bytes = byteArrayOf(0xff.toByte(), 0, 1)
        assertEquals(bytes.toList(), r.fork().body("application/json", bytes).toList())
        assertEquals("password=x", String(r.fork().body("text/plain", "password=x".toByteArray())))
    }

    @Test fun strictAndNoneLevels() {
        val strict = Redactor(RedactionLevel.STRICT, emptyList())
        assertEquals("[redacted]", strict.fork().header("X-Request-Id", "1"))
        assertEquals("v", strict.fork().header("Accept", "v"))
        assertFalse(strict.bodiesAllowed)
        val none = Redactor(RedactionLevel.NONE, emptyList())
        assertEquals("Bearer t", none.fork().header("Authorization", "Bearer t"))
        assertEquals("""{"password":"x"}""", String(none.fork().body("application/json", """{"password":"x"}""".toByteArray())))
    }
}
