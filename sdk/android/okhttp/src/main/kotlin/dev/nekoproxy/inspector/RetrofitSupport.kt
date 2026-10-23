package dev.nekoproxy.inspector

import okhttp3.Request

internal class CallMeta(val template: String, val name: String, val owner: String, val tag: String)

/**
 * Reads what Retrofit attaches to a request (an `Invocation`) to learn the route template, and
 * what `@Inspect` says about it. Safe when Retrofit is not on the classpath.
 */
internal object RetrofitSupport {
    private val available: Boolean = try { Class.forName("retrofit2.Invocation"); true } catch (e: Throwable) { false }

    fun metadata(request: Request): CallMeta? {
        if (!available) return null
        return try { Reader.read(request) } catch (e: Throwable) { null }
    }

    private object Reader {
        fun read(request: Request): CallMeta? {
            val inv = request.tag(retrofit2.Invocation::class.java) ?: return null
            val m = inv.method()
            val ins = m.getAnnotation(Inspect::class.java) ?: m.declaringClass.getAnnotation(Inspect::class.java)
            return CallMeta(template(m, request), ins?.name.orEmpty(), ins?.owner.orEmpty(), ins?.tag.orEmpty())
        }

        /** The relative path of the HTTP annotation, or null for `@Url` and unknown shapes. */
        private fun relativePath(m: java.lang.reflect.Method): String? {
            for (a in m.annotations) when (a) {
                is retrofit2.http.GET -> return a.value
                is retrofit2.http.POST -> return a.value
                is retrofit2.http.PUT -> return a.value
                is retrofit2.http.PATCH -> return a.value
                is retrofit2.http.DELETE -> return a.value
                is retrofit2.http.HEAD -> return a.value
                is retrofit2.http.OPTIONS -> return a.value
                is retrofit2.http.HTTP -> return a.path
            }
            return null
        }

        fun template(m: java.lang.reflect.Method, request: Request): String {
            val rel = relativePath(m)?.substringBefore('?') ?: return ""
            if (rel.isEmpty()) return ""
            if (rel.startsWith("/")) return rel
            // Base URL path prefix: whatever precedes the relative segments in the real path.
            val actual = request.url.encodedPathSegments.filter { it.isNotEmpty() }
            val relSegs = rel.split('/').filter { it.isNotEmpty() }
            val prefix = if (actual.size > relSegs.size) actual.take(actual.size - relSegs.size) else emptyList()
            return "/" + (prefix + relSegs).joinToString("/")
        }
    }
}
