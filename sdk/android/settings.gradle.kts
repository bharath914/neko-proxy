rootProject.name = "neko-inspector-android"

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

include(":annotations", ":core", ":okhttp", ":ktor", ":android", ":noop", ":gradle-plugin")
