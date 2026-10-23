package dev.nekoproxy.inspector.gradle

import com.android.build.api.artifact.SingleArtifact
import com.android.build.api.instrumentation.FramesComputationMode
import com.android.build.api.instrumentation.InstrumentationScope
import com.android.build.api.variant.AndroidComponentsExtension
import com.android.build.api.variant.ApplicationAndroidComponentsExtension
import org.gradle.api.GradleException
import org.gradle.api.Plugin
import org.gradle.api.Project
import java.util.Properties

class InspectorGradlePlugin : Plugin<Project> {
    override fun apply(project: Project) {
        val ext = project.extensions.create("inspector", InspectorExtension::class.java)
        ext.enabledBuildTypes.convention(setOf("debug"))
        val version = Properties().also { p ->
            InspectorGradlePlugin::class.java.getResourceAsStream("/inspector-plugin.properties")?.use { p.load(it) }
        }.getProperty("version", "0.1.0")

        val applied = project.objects.property(Boolean::class.java).convention(false)
        val manifest = project.objects.fileProperty()
        val doctor = project.tasks.register("inspectorDoctor", InspectorDoctor::class.java) {
            it.group = "help"
            it.description = "Prints what the Neko Inspector plugin did for each variant, and what to fix."
            it.applied.set(applied)
            it.report.set(ext.report)
            it.enabledBuildTypes.set(ext.enabledBuildTypes)
            it.manifest.set(manifest)
        }

        project.pluginManager.withPlugin("com.android.application") {
            applied.set(true)
            val androidComponents = project.extensions.getByType(ApplicationAndroidComponentsExtension::class.java)
            androidComponents.onVariants { variant ->
                val enabled = variant.buildType in ext.enabledBuildTypes.get()
                val config = project.dependencies
                // The annotation is harmless and must compile everywhere.
                config.add("${variant.name}Implementation", "dev.nekoproxy.inspector:inspector-annotations:$version")
                if (enabled) {
                    for (a in listOf("inspector-android")) config.add("${variant.name}Implementation", "dev.nekoproxy.inspector:$a:$version")
                    variant.instrumentation.transformClassesWith(OkHttpClassVisitorFactory::class.java, InstrumentationScope.ALL) {}
                    variant.instrumentation.setAsmFramesComputationMode(FramesComputationMode.COMPUTE_FRAMES_FOR_INSTRUMENTED_METHODS)

                    val gen = project.tasks.register("generate${variant.name.replaceFirstChar { it.uppercase() }}InspectorConfig", GenerateInspectorConfig::class.java) {
                        it.host.set(ext.host); it.port.set(ext.port); it.desktopName.set(ext.desktopName)
                        it.outputDir.set(project.layout.buildDirectory.dir("generated/inspector/${variant.name}/assets"))
                    }
                    variant.sources.assets?.addGeneratedSourceDirectory(gen, GenerateInspectorConfig::outputDir)
                    if (!manifest.isPresent) manifest.set(variant.artifacts.get(SingleArtifact.MERGED_MANIFEST))
                    ext.report.put(variant.name, "real SDK, OkHttp instrumented")
                } else {
                    config.add("${variant.name}Implementation", "dev.nekoproxy.inspector:inspector-noop:$version")
                    ext.report.put(variant.name, "no-op (no capture code in this build)")
                    // ./gradlew verifyNoInspectorInRelease: fails if real SDK code reached this variant.
                    project.tasks.register("verifyNoInspectorIn${variant.name.replaceFirstChar { it.uppercase() }}", VerifyNoInspector::class.java) {
                        it.group = "verification"
                        it.description = "Fails if the ${variant.name} APK contains real Inspector code."
                        it.apkDir.set(variant.artifacts.get(SingleArtifact.APK))
                        it.variantName.set(variant.name)
                    }
                }
            }
        }

        project.afterEvaluate {
            if (!applied.get()) {
                project.logger.warn("dev.nekoproxy.inspector: apply `com.android.application` to this project too; the Inspector plugin does nothing without it.")
            }
        }
        doctor.hashCode()
    }
}
