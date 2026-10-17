rootProject.name = "inspector-sample"

val localRepo = file("../build/local-repo")

pluginManagement {
    plugins {
        val sdkVersion = providers.gradleProperty("sdkVersion").getOrElse("0.1.0")
        id("dev.proxy.inspector") version sdkVersion
    }
    repositories {
        maven(url = file("../build/local-repo"))
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        maven(url = localRepo)
        google()
        mavenCentral()
    }
}

include(":app", ":lib")
