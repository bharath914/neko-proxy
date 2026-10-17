package dev.proxy.inspector.gradle

import org.gradle.testkit.runner.BuildResult
import org.gradle.testkit.runner.GradleRunner
import java.io.File
import java.nio.file.Files
import java.util.zip.ZipFile
import kotlin.test.Test
import kotlin.test.assertContains
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * Builds real Android apps with the plugin, against artifacts published to `build/local-repo`
 * (run `./gradlew publishAllToLocalRepo` first; `./gradlew :gradle-plugin:test` does it for you).
 */
class PluginTest {
    private val repo = System.getProperty("inspector.repo")
    private val version = System.getProperty("inspector.version")
    private val sdkHome = System.getProperty("android.home")

    private class Spec(val agp: String, val gradle: String?, val okhttp: Boolean = true, val extra: String = "", val buildTypes: String = "")

    private fun project(spec: Spec): File {
        val dir = Files.createTempDirectory("inspector-plugin-test").toFile()
        File(dir, "local.properties").writeText("sdk.dir=$sdkHome\n")
        File(dir, "gradle.properties").writeText("org.gradle.jvmargs=-Xmx1g\nandroid.useAndroidX=true\n")
        File(dir, "settings.gradle.kts").writeText(
            """
            pluginManagement { repositories { maven(url = file("$repo")); google(); mavenCentral(); gradlePluginPortal() } }
            dependencyResolutionManagement { repositories { maven(url = file("$repo")); google(); mavenCentral() } }
            rootProject.name = "t"
            include(":app")
            """.trimIndent(),
        )
        File(dir, "build.gradle.kts").writeText(
            """
            plugins {
                id("com.android.application") version "${spec.agp}" apply false
                id("dev.proxy.inspector") version "$version" apply false
            }
            """.trimIndent(),
        )
        File(dir, "app/src/main/java/t").mkdirs()
        File(dir, "app/build.gradle.kts").writeText(
            """
            plugins { id("com.android.application"); id("dev.proxy.inspector") }
            android {
                namespace = "t"
                compileSdk { version = release(37) { minorApiLevel = 1 } }
                defaultConfig { applicationId = "t.app"; minSdk = 23; targetSdk = 36; versionCode = 1; versionName = "1" }
                ${spec.buildTypes}
                compileOptions { sourceCompatibility = JavaVersion.VERSION_11; targetCompatibility = JavaVersion.VERSION_11 }
            }
            ${spec.extra}
            dependencies { ${if (spec.okhttp) """implementation("com.squareup.okhttp3:okhttp:5.5.0")""" else ""} }
            """.trimIndent(),
        )
        File(dir, "app/src/main/AndroidManifest.xml").writeText(
            """<manifest xmlns:android="http://schemas.android.com/apk/res/android"><uses-permission android:name="android.permission.INTERNET"/><application/></manifest>""",
        )
        File(dir, "app/src/main/java/t/Main.java").writeText(
            if (spec.okhttp) {
                """
                package t;
                public class Main {
                    public static okhttp3.OkHttpClient make() { return new okhttp3.OkHttpClient.Builder().build(); }
                    public static okhttp3.OkHttpClient plain() { return new okhttp3.OkHttpClient(); }
                }
                """.trimIndent()
            } else "package t; public class Main {}",
        )
        return dir
    }

    private fun run(dir: File, spec: Spec, vararg args: String): BuildResult {
        val r = GradleRunner.create().withProjectDir(dir).withArguments(*args, "--stacktrace", "--console=plain").forwardOutput()
        spec.gradle?.let { r.withGradleVersion(it) }
        return r.build()
    }

    private fun dexStrings(apk: File): String {
        ZipFile(apk).use { z ->
            return z.entries().asSequence().filter { it.name.endsWith(".dex") }.joinToString("\n") { String(z.getInputStream(it).readBytes(), Charsets.ISO_8859_1) }
        }
    }

    private fun apk(dir: File, type: String): File = File(dir, "app/build/outputs/apk/$type").walkTopDown().first { it.extension == "apk" }

    private fun checkDebugAndRelease(spec: Spec) {
        val dir = project(spec)
        run(dir, spec, ":app:assembleDebug", ":app:assembleRelease")
        val debug = dexStrings(apk(dir, "debug"))
        val release = dexStrings(apk(dir, "release"))
        assertContains(debug, "Ldev/proxy/inspector/internal/Engine;", message = "debug has the real SDK")
        assertContains(debug, "Ldev/proxy/inspector/InspectorOkHttp;")
        assertFalse("Ldev/proxy/inspector/internal/" in release, "release must not contain the real SDK")
        assertFalse("_proxyinspect" in release)
        ZipFile(apk(dir, "release")).use { z -> assertFalse(z.entries().asSequence().any { it.name == "assets/inspector.properties" }, "no config asset in release") }
        assertTrue(ZipFile(apk(dir, "debug")).use { z -> z.getEntry("assets/inspector.properties") != null })
    }

    @Test
    fun debugGetsTheRealSdkAndReleaseOnlyTheNoop_AGP9() = checkDebugAndRelease(Spec("9.4.1", null))

    @Test
    fun debugGetsTheRealSdkAndReleaseOnlyTheNoop_AGP8() = checkDebugAndRelease(Spec("8.13.1", "8.14"))

    @Test
    fun theOkHttpConstructorIsInstrumentedExactlyOnce() {
        val spec = Spec("9.4.1", null)
        val dir = project(spec)
        run(dir, spec, ":app:assembleDebug")
        val tmp = Files.createTempDirectory("dex").toFile()
        ZipFile(apk(dir, "debug")).use { z ->
            z.entries().asSequence().filter { it.name.endsWith(".dex") }.forEach { File(tmp, it.name).writeBytes(z.getInputStream(it).readBytes()) }
        }
        val dexdump = File(sdkHome, "build-tools").listFiles()!!.sortedBy { it.name }.last().resolve("dexdump")
        val out = ProcessBuilder(dexdump.absolutePath, "-d", *tmp.listFiles()!!.map { it.absolutePath }.toTypedArray()).start().inputStream.bufferedReader().readText()
        val calls = Regex("invoke-static [^\\n]*InspectorOkHttp;\\.install").findAll(out).count()
        assertEquals(1, calls, "one injected call, in OkHttpClient.<init>")
        val initBlock = out.substringAfter("okhttp3.OkHttpClient.<init>:(Lokhttp3/OkHttpClient\$Builder;)V").lines().take(4).joinToString("\n")
        assertContains(initBlock, "InspectorOkHttp;.install", message = "the call is the first thing the constructor does")
    }

    @Test
    fun anAppWithoutOkHttpStillBuildsAndConfigurationCacheIsReused() {
        val spec = Spec("9.4.1", null, okhttp = false)
        val dir = project(spec)
        run(dir, spec, ":app:assembleDebug", "--configuration-cache")
        val second = run(dir, spec, ":app:assembleDebug", "--configuration-cache")
        assertContains(second.output, "Reusing configuration cache")
    }

    @Test
    fun extensionChoosesBuildTypesAndWritesTheManualHost() {
        val spec = Spec(
            "9.4.1", null,
            extra = """inspector { enabledBuildTypes = setOf("debug", "staging"); host = "192.168.0.9"; port = 9000; desktopName = "Asha's Mac" }""",
            buildTypes = """buildTypes { create("staging") { initWith(getByName("release")) ; signingConfig = signingConfigs.getByName("debug") } }""",
        )
        val dir = project(spec)
        run(dir, spec, ":app:assembleStaging", ":app:assembleRelease")
        assertContains(dexStrings(apk(dir, "staging")), "Ldev/proxy/inspector/internal/Engine;")
        assertFalse("Ldev/proxy/inspector/internal/" in dexStrings(apk(dir, "release")))
        val props = ZipFile(apk(dir, "staging")).use { z -> String(z.getInputStream(z.getEntry("assets/inspector.properties")).readBytes()) }
        assertContains(props, "host=192.168.0.9"); assertContains(props, "port=9000"); assertContains(props, "desktopName=Asha's Mac")
    }

    @Test
    fun doctorSaysWhatHappened() {
        val spec = Spec("9.4.1", null)
        val dir = project(spec)
        val out = run(dir, spec, ":app:inspectorDoctor").output
        assertContains(out, "Proxy Inspector doctor")
    }
}
