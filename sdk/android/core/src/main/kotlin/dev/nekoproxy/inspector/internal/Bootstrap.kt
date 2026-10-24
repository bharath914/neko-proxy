package dev.nekoproxy.inspector.internal

import dev.nekoproxy.inspector.Inspector

/** Starts and stops the SDK. Called by the Android layer (and by tests); apps never call this. */
object Bootstrap {
    @JvmStatic
    fun start(platform: Platform) {
        val engine = Runtime.engine ?: Engine(platform) { Inspector.settings() }.also { Runtime.engine = it }
        engine.start()
    }

    @JvmStatic
    fun stop() {
        Runtime.engine?.stop()
        Runtime.engine = null
    }
}
