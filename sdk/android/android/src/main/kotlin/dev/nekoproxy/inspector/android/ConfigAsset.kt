package dev.nekoproxy.inspector.android

import android.content.Context
import dev.nekoproxy.inspector.Inspector
import java.util.Properties

/** Reads `assets/inspector.properties`, which the Gradle plugin writes from its `inspector { }` extension. */
internal object ConfigAsset {
    fun apply(context: Context) {
        val p = Properties()
        try { context.assets.open("inspector.properties").use { p.load(it) } } catch (e: Exception) { return }
        val host = p.getProperty("host")?.takeIf { it.isNotBlank() }
        val port = p.getProperty("port")?.toIntOrNull()
        val name = p.getProperty("desktopName")?.takeIf { it.isNotBlank() }
        if (host == null && name == null && port == null) return
        Inspector.configure {
            this.host = host
            if (port != null) this.port = port
            this.desktopName = name
        }
    }
}
