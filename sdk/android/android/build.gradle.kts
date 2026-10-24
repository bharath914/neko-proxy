plugins {
    alias(libs.plugins.android.library)
    `maven-publish`
}

android {
    namespace = "dev.nekoproxy.inspector.android"
    compileSdk = 35
    defaultConfig {
        minSdk = 23
        consumerProguardFiles("consumer-rules.pro")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
    publishing { singleVariant("release") { withSourcesJar() } }
}

dependencies {
    api("org.jetbrains.kotlin:kotlin-stdlib:2.0.21")
    api(project(":core"))
    api(project(":okhttp"))
    api(project(":ktor"))
    implementation(libs.androidx.startup)
    // Fragment screen tracking is used only if the app has fragments.
    compileOnly("androidx.fragment:fragment:1.8.9")
}

publishing {
    publications {
        register<MavenPublication>("release") {
            artifactId = "inspector-android"
            afterEvaluate { from(components["release"]) }
        }
    }
}
