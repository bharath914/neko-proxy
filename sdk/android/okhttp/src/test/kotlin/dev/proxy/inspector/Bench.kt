package dev.proxy.inspector

import dev.proxy.inspector.internal.Bootstrap
import dev.proxy.inspector.internal.Recorder
import dev.proxy.inspector.internal.Runtime
import dev.proxy.inspector.testing.TestCore
import dev.proxy.inspector.testing.TestPlatform
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.lang.management.ManagementFactory
import java.util.concurrent.TimeUnit

/**
 * `./gradlew :okhttp:bench` : the scenarios of docs/sdk/docs/09-testing-benchmarks.md on the JVM, against
 * `sdk/test-server/server.py` and `sdk-test-core`. JVM numbers, not device numbers: see docs/sdk-bench.md.
 */
fun main() {
    val root = File(System.getProperty("repo.root"))
    val server = ProcessBuilder("python3", File(root, "sdk/test-server/server.py").path, "--port", "18081").redirectErrorStream(true).start()
    Thread.sleep(800)
    val base = "http://127.0.0.1:18081"
    val core = TestCore.start("--auto-allow")
    val results = mutableListOf<String>()
    val os = ManagementFactory.getOperatingSystemMXBean() as com.sun.management.OperatingSystemMXBean

    fun client(withSdk: Boolean) = OkHttpClient.Builder().apply {
        if (withSdk) { addInterceptor(InspectorInterceptor()); eventListenerFactory(InspectorEventListener.Factory(null)) }
        connectTimeout(5, TimeUnit.SECONDS); readTimeout(30, TimeUnit.SECONDS)
    }.build()

    fun percentile(sorted: List<Long>, p: Double) = sorted[((sorted.size - 1) * p).toInt()] / 1000.0

    fun scenario(name: String, c: OkHttpClient, path: String, n: Int, perSecond: Int?, parallel: Int = 1, note: String = "") {
        val lat = java.util.Collections.synchronizedList(ArrayList<Long>())
        val bytesBefore = Runtime.engine?.bytesSent?.get() ?: 0
        val droppedBefore = Runtime.engine?.dropped() ?: 0
        System.gc()
        val heapBefore = java.lang.Runtime.getRuntime().let { it.totalMemory() - it.freeMemory() }
        val cpuBefore = os.processCpuTime
        val t0 = System.nanoTime()
        val gap = perSecond?.let { 1_000_000_000L / it }
        val pool = java.util.concurrent.Executors.newFixedThreadPool(parallel)
        val latch = java.util.concurrent.CountDownLatch(n)
        for (i in 0 until n) {
            if (gap != null) { val due = t0 + i * gap; while (System.nanoTime() < due) Thread.sleep(0, 200_000) }
            pool.execute {
                val s = System.nanoTime()
                try { c.newCall(Request.Builder().url(base + path).build()).execute().use { it.body.bytes() } } catch (e: Exception) { }
                lat += (System.nanoTime() - s) / 1000
                latch.countDown()
            }
        }
        latch.await(120, TimeUnit.SECONDS)
        pool.shutdown()
        val wall = (System.nanoTime() - t0) / 1e9
        Thread.sleep(500)
        val cpu = (os.processCpuTime - cpuBefore) / 1e9
        val heap = java.lang.Runtime.getRuntime().let { it.totalMemory() - it.freeMemory() } - heapBefore
        val sorted = lat.sorted()
        val sent = (Runtime.engine?.bytesSent?.get() ?: 0) - bytesBefore
        val dropped = (Runtime.engine?.dropped() ?: 0) - droppedBefore
        results += "| $name | ${"%.2f".format(percentile(sorted, 0.5))} | ${"%.2f".format(percentile(sorted, 0.99))} | ${"%.0f".format(cpu / wall * 100)}% | ${heap / 1024} KB | $sent | $dropped | $note |"
        println(results.last())
    }

    fun connect() {
        Bootstrap.stop()
        Bootstrap.start(TestPlatform(ports = { listOf(core.port) }))
        val end = System.currentTimeMillis() + 10_000
        while (!Recorder.active() && System.currentTimeMillis() < end) Thread.sleep(20)
        check(Recorder.active()) { "SDK did not connect" }
    }

    // Warm up the JIT and the connection pool so the first scenario is not penalised.
    val warm = client(false)
    repeat(300) { warm.newCall(Request.Builder().url("$base/json").build()).execute().use { it.body.bytes() } }

    scenario("1 baseline, no SDK", client(false), "/json", 600, 50)
    scenario("1b baseline 100 KB, no SDK", client(false), "/json/100k", 300, 50)
    scenario("1c baseline 5 MB, no SDK", client(false), "/json/5m", 10, null)
    scenario("1d baseline burst of 1,000, no SDK", client(false), "/json", 1000, null, parallel = 16)
    Bootstrap.stop()
    scenario("2 SDK present, no desktop connected", client(true), "/json", 600, 50)
    connect()
    scenario("3 connected, 1 KB JSON, 50 req/s", client(true), "/json", 600, 50)
    scenario("4 connected, 100 KB JSON, 50 req/s", client(true), "/json/100k", 300, 50)
    scenario("5 connected, 5 MB download", client(true), "/json/5m", 10, null, note = "over the cap: kept on the device")
    scenario("6 burst of 1,000 requests", client(true), "/json", 1000, null, parallel = 16, note = "as fast as 16 threads go")
    // 7: a 10 s event stream (the plan says 60 s; the shape is the same and the full run is a manual check).
    scenario("7 streaming response, 10 s", client(true), "/sse?n=100&ms=100", 1, null, note = "SSE, 100 events")
    // 8: the desktop stalls. SIGSTOP the ingest process; the app must not notice.
    val pid = ProcessHandle.allProcesses().filter { it.info().command().orElse("").endsWith("sdk-test-core") }.findFirst().get().pid()
    ProcessBuilder("kill", "-STOP", pid.toString()).start().waitFor()
    scenario("8 desktop stalled", client(true), "/json", 600, 50, note = "ingest process stopped")
    ProcessBuilder("kill", "-CONT", pid.toString()).start().waitFor()

    println("\n| Scenario | p50 ms | p99 ms | CPU | heap delta | bytes to desktop | dropped events | note |\n|---|---|---|---|---|---|---|---|")
    results.forEach(::println)
    File(System.getProperty("bench.out") ?: "/tmp/sdk-bench-android.md").writeText(
        "| Scenario | p50 ms | p99 ms | CPU | heap delta | bytes to desktop | dropped events | note |\n|---|---|---|---|---|---|---|---|\n" + results.joinToString("\n") + "\n",
    )
    Bootstrap.stop(); core.close(); server.destroy()
    kotlin.system.exitProcess(0)
}
