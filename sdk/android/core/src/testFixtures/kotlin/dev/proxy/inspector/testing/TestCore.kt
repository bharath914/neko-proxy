package dev.proxy.inspector.testing

import dev.proxy.inspector.internal.Candidate
import dev.proxy.inspector.internal.LogLevel
import dev.proxy.inspector.internal.Platform
import org.json.JSONObject
import java.io.BufferedReader
import java.io.File
import java.io.InputStreamReader
import java.nio.file.Files
import java.util.concurrent.CopyOnWriteArrayList

/** A running `sdk-test-core`, the Rust ingest server every SDK test talks to. */
class TestCore private constructor(private val proc: Process, val port: Int, val record: File, val dir: File) : AutoCloseable {
    fun lines(): List<JSONObject> =
        if (!record.exists()) emptyList() else record.readLines().filter { it.isNotBlank() }.map { JSONObject(it) }

    fun ofType(type: String) = lines().filter { it.optString("type") == type }

    fun waitFor(what: String, timeoutMs: Long = 15_000, cond: (List<JSONObject>) -> Boolean): List<JSONObject> {
        val end = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < end) {
            val l = lines()
            if (cond(l)) return l
            Thread.sleep(25)
        }
        throw AssertionError("timed out waiting for $what; recorded:\n${lines().joinToString("\n")}")
    }

    override fun close() {
        proc.destroy()
        proc.waitFor()
        dir.deleteRecursively()
    }

    companion object {
        val binary: File? = System.getProperty("sdk.testcore")?.let(::File)?.takeIf { it.canExecute() }

        fun start(vararg args: String, port: Int = 0, dir: File = Files.createTempDirectory("sdk-test-core").toFile()): TestCore {
            val bin = binary ?: throw AssertionError("sdk-test-core not built: run `cargo build -p sdk --bins` (or pass -PsdkTestCore=...)")
            val record = File(dir, "out.jsonl")
            val cmd = listOf(bin.absolutePath, "--port", port.toString(), "--record", record.absolutePath, "--data-dir", File(dir, "state").absolutePath) + args
            val p = ProcessBuilder(cmd).redirectError(ProcessBuilder.Redirect.DISCARD).start()
            val ready = JSONObject(BufferedReader(InputStreamReader(p.inputStream)).readLine())
            return TestCore(p, ready.getInt("port"), record, dir)
        }
    }
}

/** An in-memory [Platform] for JVM tests. */
open class TestPlatform(
    override val storageDir: File = Files.createTempDirectory("inspector-state").toFile(),
    private val ports: () -> List<Int> = { emptyList() },
    override val appId: String = "com.demo.shop",
) : Platform {
    override val appName = "Demo Shop"
    override val appVersion = "1.2.3"
    override val build = "42"
    override val deviceName = "Test Pixel"
    override val deviceModel = "JVM"
    override val osVersion = "Android 15"
    val logs = CopyOnWriteArrayList<String>()
    val notices = CopyOnWriteArrayList<String>()

    override fun localCandidates() = ports().map { Candidate("127.0.0.1", it, "usb") }
    override fun log(level: LogLevel, message: String) { logs += "$level $message" }
    override fun notify(message: String) { notices += message }
}
