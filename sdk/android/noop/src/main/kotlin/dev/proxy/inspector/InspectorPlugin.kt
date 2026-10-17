package dev.proxy.inspector

import io.ktor.client.plugins.api.createClientPlugin
import io.ktor.client.request.HttpRequestBuilder
import io.ktor.util.AttributeKey

val InspectName = AttributeKey<String>("dev.proxy.inspector.name")
val InspectOwner = AttributeKey<String>("dev.proxy.inspector.owner")
val InspectTag = AttributeKey<String>("dev.proxy.inspector.tag")
val InspectRoute = AttributeKey<String>("dev.proxy.inspector.route")

fun HttpRequestBuilder.inspect(name: String = "", owner: String = "", tag: String = "", route: String = "") {}

/** Installs, and does nothing. */
val InspectorPlugin = createClientPlugin("Inspector") {}
