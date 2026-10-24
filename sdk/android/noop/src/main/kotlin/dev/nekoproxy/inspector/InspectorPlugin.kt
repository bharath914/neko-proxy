package dev.nekoproxy.inspector

import io.ktor.client.plugins.api.createClientPlugin
import io.ktor.client.request.HttpRequestBuilder
import io.ktor.util.AttributeKey

val InspectName = AttributeKey<String>("dev.nekoproxy.inspector.name")
val InspectOwner = AttributeKey<String>("dev.nekoproxy.inspector.owner")
val InspectTag = AttributeKey<String>("dev.nekoproxy.inspector.tag")
val InspectRoute = AttributeKey<String>("dev.nekoproxy.inspector.route")

fun HttpRequestBuilder.inspect(name: String = "", owner: String = "", tag: String = "", route: String = "") {}

/** Installs, and does nothing. */
val InspectorPlugin = createClientPlugin("Inspector") {}
