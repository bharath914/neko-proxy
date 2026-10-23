buildscript {
    repositories { google(); mavenCentral() }
    // AGP 9 has Kotlin built in, at 2.2. This app's dependencies (Ktor 3.6, OkHttp 5.5) are newer, so it asks for 2.4.
    dependencies { classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.4.21") }
}

plugins {
    id("com.android.application") version "9.4.1" apply false
    id("com.android.library") version "9.4.1" apply false
    id("dev.nekoproxy.inspector") apply false
}
