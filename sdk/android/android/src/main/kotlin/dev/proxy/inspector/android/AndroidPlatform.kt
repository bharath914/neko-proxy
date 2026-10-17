package dev.proxy.inspector.android

import android.annotation.SuppressLint
import android.app.Application
import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.util.Log
import android.widget.Toast
import dev.proxy.inspector.internal.Candidate
import dev.proxy.inspector.internal.LogLevel
import dev.proxy.inspector.internal.Platform
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

internal class AndroidPlatform(private val app: Application) : Platform {
    private val pm = app.packageManager
    private val info = runCatching { pm.getPackageInfo(app.packageName, 0) }.getOrNull()

    override val appId: String = app.packageName
    override val appName: String = runCatching { app.applicationInfo.loadLabel(pm).toString() }.getOrDefault(app.packageName)
    override val appVersion: String = info?.versionName.orEmpty()
    @Suppress("DEPRECATION")
    override val build: String = info?.let { if (Build.VERSION.SDK_INT >= 28) it.longVersionCode.toString() else it.versionCode.toString() }.orEmpty()
    override val deviceName: String = runCatching { Settings.Global.getString(app.contentResolver, "device_name") }.getOrNull()?.takeIf { it.isNotBlank() } ?: Build.MODEL
    override val deviceModel: String = "${Build.MANUFACTURER} ${Build.MODEL}".trim()
    override val osVersion: String = "Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})"
    override val storageDir: File = app.noBackupFilesDir

    private val isEmulator: Boolean = Build.FINGERPRINT.startsWith("generic") || Build.FINGERPRINT.contains("emulator") ||
        Build.HARDWARE.contains("goldfish") || Build.HARDWARE.contains("ranchu") || Build.MODEL.contains("Emulator") ||
        Build.MODEL.contains("sdk_gphone") || Build.PRODUCT.contains("sdk")

    /** Emulator host alias first, then localhost (reached over `adb reverse`). Port 0 means the configured port. */
    override fun localCandidates(): List<Candidate> = buildList {
        if (isEmulator) add(Candidate("10.0.2.2", 0, "emulator"))
        add(Candidate("localhost", 0, "usb"))
    }

    @SuppressLint("NewApi")
    override fun discover(timeoutMs: Long): List<Candidate> {
        val nsd = app.getSystemService(Context.NSD_SERVICE) as? NsdManager ?: return emptyList()
        val found = CopyOnWriteArrayList<Candidate>()
        val resolvers = CopyOnWriteArrayList<CountDownLatch>()
        val listener = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) {}
            override fun onServiceFound(s: NsdServiceInfo) {
                val done = CountDownLatch(1).also { resolvers += it }
                @Suppress("DEPRECATION")
                nsd.resolveService(s, object : NsdManager.ResolveListener {
                    override fun onResolveFailed(si: NsdServiceInfo, errorCode: Int) { done.countDown() }
                    override fun onServiceResolved(si: NsdServiceInfo) {
                        val txt = si.attributes
                        @Suppress("DEPRECATION")
                        val host = si.host?.hostAddress
                        if (host != null) found += Candidate(
                            host, si.port, "wifi",
                            fingerprintShort = txt["fp"]?.toString(Charsets.UTF_8),
                            desktopName = txt["name"]?.toString(Charsets.UTF_8) ?: si.serviceName,
                        )
                        done.countDown()
                    }
                })
            }
            override fun onServiceLost(s: NsdServiceInfo) {}
            override fun onDiscoveryStopped(serviceType: String) {}
            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {}
            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {}
        }
        try {
            nsd.discoverServices("_proxyinspect._tcp", NsdManager.PROTOCOL_DNS_SD, listener)
            Thread.sleep(timeoutMs)
            resolvers.forEach { it.await(500, TimeUnit.MILLISECONDS) }
        } catch (e: Exception) {
        } finally {
            runCatching { nsd.stopServiceDiscovery(listener) }
        }
        return found.distinctBy { it.host to it.port }
    }

    override fun log(level: LogLevel, message: String) {
        when (level) {
            LogLevel.DEBUG -> Log.d(TAG, message)
            LogLevel.INFO -> Log.i(TAG, message)
            LogLevel.WARN -> Log.w(TAG, message)
            LogLevel.ERROR -> Log.e(TAG, message)
        }
    }

    override fun notify(message: String) {
        Handler(Looper.getMainLooper()).post { runCatching { Toast.makeText(app, message, Toast.LENGTH_LONG).show() } }
    }

    companion object { const val TAG = "Inspector" }
}
