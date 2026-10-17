plugins {
    alias(libs.plugins.kotlin.jvm) apply false
    alias(libs.plugins.android.library) apply false
}

// Every artifact shares one group and version. Change both in one place.
allprojects {
    group = "dev.proxy.inspector"
    version = "0.1.0"
}
