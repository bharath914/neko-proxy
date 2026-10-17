package dev.proxy.inspector.internal

import java.io.File

/** A desktop the SDK might connect to. */
data class Candidate(
    val host: String,
    val port: Int,
    /** "emulator", "usb", "wifi", or "" when unknown. Reported in Hello. */
    val kind: String = "",
    /** From mDNS TXT records; null for addresses that were not discovered. */
    val fingerprintShort: String? = null,
    val desktopName: String? = null,
)

/**
 * Everything that differs between Android and a plain JVM (tests, desktop tools). The SDK core is
 * written against this so almost all of it runs in JVM unit tests.
 */
interface Platform {
    val appId: String
    val appName: String
    val appVersion: String
    val build: String
    val deviceName: String
    val deviceModel: String
    val osVersion: String
    val sdkName: String get() = "android"

    /** App-private directory excluded from backup. Holds the pairing token, nothing else. */
    val storageDir: File

    /** Fixed candidates in priority order, e.g. the emulator host alias and localhost for adb reverse. */
    fun localCandidates(): List<Candidate>

    /** Look for desktops on the LAN for up to [timeoutMs]. May return nothing. */
    fun discover(timeoutMs: Long): List<Candidate> = emptyList()

    fun log(level: LogLevel, message: String)

    /** A short, non-blocking on-screen note (a Toast on Android). May do nothing. */
    fun notify(message: String) {}
}

enum class LogLevel { DEBUG, INFO, WARN, ERROR }
