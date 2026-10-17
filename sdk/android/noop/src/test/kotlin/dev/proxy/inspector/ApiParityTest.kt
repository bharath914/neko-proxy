package dev.proxy.inspector

import java.io.File
import java.net.URLClassLoader
import java.util.jar.JarFile
import kotlin.reflect.KClass
import kotlin.reflect.KVisibility
import kotlin.reflect.full.declaredFunctions
import kotlin.reflect.full.declaredMemberProperties
import kotlin.reflect.jvm.kotlinFunction
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue

/** The release artifact must offer exactly what the debug one does, or release builds would not compile. */
class ApiParityTest {
    private fun jars(prop: String) = System.getProperty(prop).split(File.pathSeparator).map(::File)

    /** Public classes of `dev.proxy.inspector` (not the `internal` package) and their public members. */
    private fun surface(jars: List<File>): Set<String> {
        // A loader of its own: the test classpath contains the no-op classes, which would shadow the real ones.
        val deps = System.getProperty("java.class.path").split(File.pathSeparator).filter { "/noop/build/" !in it }.map(::File)
        val loader = URLClassLoader((jars + deps).map { it.toURI().toURL() }.toTypedArray(), ClassLoader.getPlatformClassLoader())
        val out = sortedSetOf<String>()
        for (jar in jars) JarFile(jar).use { jf ->
            for (e in jf.entries()) {
                val n = e.name
                if (!n.endsWith(".class") || !n.startsWith("dev/proxy/inspector/") || n.startsWith("dev/proxy/inspector/internal/")) continue
                if (n.substringAfterLast("/").contains("$") || n == "module-info.class") continue
                val c = runCatching { loader.loadClass(n.removeSuffix(".class").replace('/', '.')) }.getOrNull() ?: continue
                describe(c, out)
            }
        }
        return out
    }

    private fun describe(c: Class<*>, out: MutableSet<String>) {
        val isFileFacade = c.getAnnotation(Metadata::class.java)?.kind == 2
        if (!isFileFacade) {
            val k: KClass<*> = c.kotlin
            val v = runCatching { k.visibility }.getOrNull()
            if (v != KVisibility.PUBLIC) return
            out += "class ${c.name}"
            for (f in k.declaredFunctions) if (f.visibility == KVisibility.PUBLIC) out += "fun ${c.name}.${f.name}(${f.parameters.drop(1).joinToString { it.type.toString() }}): ${f.returnType}"
            for (p in k.declaredMemberProperties) if (p.visibility == KVisibility.PUBLIC) out += "prop ${c.name}.${p.name}: ${p.returnType}"
            for (ctor in c.declaredConstructors) if (java.lang.reflect.Modifier.isPublic(ctor.modifiers)) out += "ctor ${c.name}(${ctor.parameterTypes.joinToString { it.name }})"
        } else {
            // Top-level declarations of a file: Kotlin-visible public ones only.
            for (m in c.declaredMethods) {
                if (!java.lang.reflect.Modifier.isPublic(m.modifiers) || m.isSynthetic) continue
                val kf = runCatching { m.kotlinFunction }.getOrNull()
                if (kf != null && kf.visibility != KVisibility.PUBLIC) continue
                if (m.name.contains('$')) continue
                out += "toplevel ${m.name}(${m.parameterTypes.joinToString { it.name }})"
            }
        }
    }

    @Test
    fun theReleaseArtifactHasTheSamePublicApiAsTheDebugOnes() {
        val real = surface(jars("jars.real"))
        val noop = surface(jars("jars.noop"))
        assertTrue(real.size > 15, "found too little API to compare: $real")
        assertEquals(real, noop, "only in real: ${real - noop}\nonly in noop: ${noop - real}")
    }

    @Test
    fun theComparisonFailsWhenAnApiDiffers() {
        // Self-check: the machinery must notice a missing member.
        val real = surface(jars("jars.real"))
        val broken = real.filterNot { it.contains("setScreen") }.toSet()
        assertNotEquals(real, broken)
        assertTrue(real.any { it.contains("setScreen") }, "the real API should have setScreen")
    }
}
