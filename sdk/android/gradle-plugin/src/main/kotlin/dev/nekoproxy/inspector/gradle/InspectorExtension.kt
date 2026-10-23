package dev.nekoproxy.inspector.gradle

import org.gradle.api.provider.MapProperty
import org.gradle.api.provider.Property
import org.gradle.api.provider.SetProperty

/**
 * ```
 * inspector {
 *     enabledBuildTypes = setOf("debug", "staging")   // default: debuggable build types
 *     host = "192.168.1.24"                           // optional: skip discovery
 * }
 * ```
 */
abstract class InspectorExtension {
    /** Build types that get the real SDK. Others get the no-op. Default: `debug`. */
    abstract val enabledBuildTypes: SetProperty<String>

    /** A desktop to connect to directly, when discovery cannot find it. */
    abstract val host: Property<String>
    abstract val port: Property<Int>

    /** Pick one desktop by name when several are on the network. */
    abstract val desktopName: Property<String>

    /** What the plugin did, per variant. Read by `inspectorDoctor`. */
    internal abstract val report: MapProperty<String, String>
}
