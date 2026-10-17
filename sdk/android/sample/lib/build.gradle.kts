plugins { id("com.android.library") }

android {
    namespace = "dev.proxy.samplelib"
    compileSdk { version = release(37) { minorApiLevel = 1 } }
    defaultConfig { minSdk = 23 }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_11; targetCompatibility = JavaVersion.VERSION_11 }
}

dependencies { implementation("com.squareup.okhttp3:okhttp:5.5.0") }
