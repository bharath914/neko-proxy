rootProject.name = "inspector-sample"

val localRepo = file("../build/local-repo")

pluginManagement {
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
