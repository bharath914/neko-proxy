package dev.proxy.inspector

import okhttp3.EventListener
import okhttp3.OkHttpClient

/**
 * Hooks used by the Gradle plugin's instrumentation: adds the interceptor and the timing listener
 * to a client being built, once, and leaves anything the app set up alone.
 */
object InspectorOkHttp {
    @JvmStatic
    fun install(builder: OkHttpClient.Builder) {
        if (builder.interceptors().none { it is InspectorInterceptor }) builder.addInterceptor(InspectorInterceptor())
        composeListener(builder)
    }

    private fun composeListener(builder: OkHttpClient.Builder) {
        try {
            val f = OkHttpClient.Builder::class.java.getDeclaredField("eventListenerFactory").apply { isAccessible = true }
            val current = f.get(builder) as EventListener.Factory
            if (current is InspectorEventListener.Factory) return
            builder.eventListenerFactory(InspectorEventListener.Factory(current))
        } catch (e: Throwable) {
            // Can't read the existing factory on this OkHttp build; only install ours if there is none to lose.
            // Timings then reduce to the total, which the interceptor measures itself.
        }
    }
}
