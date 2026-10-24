package dev.nekoproxy.inspector

import okhttp3.Interceptor
import okhttp3.OkHttpClient
import okhttp3.Response

/** Passes every call straight through. */
class InspectorInterceptor : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response = chain.proceed(chain.request())
}

object InspectorOkHttp {
    @JvmStatic fun install(builder: OkHttpClient.Builder) {}
}
