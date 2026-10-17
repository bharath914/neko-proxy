package dev.proxy.samplelib

import okhttp3.OkHttpClient
import okhttp3.Request

/** A client built inside a dependency of the app. The plugin instruments it too. */
object LibraryClient {
    private val client = OkHttpClient()

    fun fetch(url: String): Int = client.newCall(Request.Builder().url(url).build()).execute().use { it.code }
}
