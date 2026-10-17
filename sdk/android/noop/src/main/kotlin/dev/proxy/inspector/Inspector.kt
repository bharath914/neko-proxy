package dev.proxy.inspector

/** Optional settings. Does nothing in a release build. */
class InspectorConfig internal constructor() {
    var host: String? = null
    var port: Int = 8897
    var desktopName: String? = null
}

/** Release-build stand-in for the real Inspector: the same API, no sockets, no discovery, no threads. */
object Inspector {
    @JvmStatic fun configure(block: InspectorConfig.() -> Unit) {}
    @JvmStatic fun setScreen(name: String) {}
    @JvmStatic fun desktop(name: String) {}
    @JvmStatic fun forgetPairing() {}
}
