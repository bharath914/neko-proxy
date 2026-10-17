package dev.proxy.inspector

import dev.proxy.inspector.internal.Engine
import dev.proxy.inspector.internal.Runtime

/** Optional settings. Everything works without calling [Inspector.configure]. */
class InspectorConfig internal constructor() {
    /** A desktop to connect to directly, when automatic discovery cannot find it. */
    var host: String? = null

    /** Port of the desktop's SDK listener. */
    var port: Int = Engine.DEFAULT_PORT

    /** Pick one desktop by name when several are found (an office network). */
    var desktopName: String? = null
}

/**
 * Streams this app's network calls to the Proxy desktop app. Debug builds only: release builds
 * get a no-op with this same API.
 */
object Inspector {
    @Volatile private var config = InspectorConfig()

    /** Optional. Call before the first request, for example in `Application.onCreate`. */
    @JvmStatic
    fun configure(block: InspectorConfig.() -> Unit) {
        val c = InspectorConfig().apply { block() }
        config = c
    }

    /** Names the screen shown on the calls made from now on. Use it for Compose and other screens the SDK cannot see. */
    @JvmStatic
    fun setScreen(name: String) {
        Runtime.engine?.screen = name
    }

    /** Connect only to the desktop with this name, when several are on the network. */
    @JvmStatic
    fun desktop(name: String) {
        config = InspectorConfig().also { it.host = config.host; it.port = config.port; it.desktopName = name }
    }

    /**
     * Forget the pairing with the desktop. The next connection asks for approval again. Needed after
     * the desktop's certificate was replaced.
     */
    @JvmStatic
    fun forgetPairing() {
        Runtime.engine?.forgetPairing()
    }

    internal fun settings() = Engine.EngineSettings(config.host, config.port, config.desktopName)
}
