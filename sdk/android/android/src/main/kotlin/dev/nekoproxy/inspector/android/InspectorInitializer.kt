package dev.nekoproxy.inspector.android

import android.app.Application
import android.content.Context
import androidx.startup.Initializer
import dev.nekoproxy.inspector.internal.Bootstrap

/** Runs at process start through AndroidX App Startup. Registers callbacks and starts the SDK's own thread; nothing heavy. */
class InspectorInitializer : Initializer<Unit> {
    override fun create(context: Context) {
        val app = context.applicationContext as Application
        val platform = AndroidPlatform(app)
        ConfigAsset.apply(app)
        app.registerActivityLifecycleCallbacks(ScreenTracker())
        // Connecting happens on the SDK's background thread: reading the stored pairing, trying the
        // emulator and USB addresses, then (only if needed) browsing for the desktop on the LAN.
        Bootstrap.start(platform)
    }

    override fun dependencies(): List<Class<out Initializer<*>>> = emptyList()
}
