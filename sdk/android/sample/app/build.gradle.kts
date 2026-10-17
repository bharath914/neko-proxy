plugins {
    id("com.android.application")
    id("dev.proxy.inspector")
}

android {
    namespace = "dev.proxy.sample"
    compileSdk { version = release(37) { minorApiLevel = 1 } }
    defaultConfig {
        applicationId = "dev.proxy.sample"
        minSdk = 23
        targetSdk = 36
        versionCode = 1
        versionName = "1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    buildTypes {
        release { isMinifyEnabled = true; proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro") ; signingConfig = signingConfigs.getByName("debug") }
    }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_11; targetCompatibility = JavaVersion.VERSION_11 }
}

// Nothing else is needed for capture: the plugin adds the SDK to debug and the no-op to release.
inspector {
    // host = "192.168.0.109"   // only if discovery cannot find the desktop
}

dependencies {
    implementation(project(":lib"))
    implementation("com.squareup.okhttp3:okhttp:5.5.0")
    implementation("com.squareup.retrofit2:retrofit:3.0.0")
    implementation("io.ktor:ktor-client-core:3.6.0")
    implementation("io.ktor:ktor-client-okhttp:3.6.0")
    implementation("io.ktor:ktor-client-cio:3.6.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
}
