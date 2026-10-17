plugins {
    alias(libs.plugins.kotlin.jvm)
    `maven-publish`
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11) } }
java { withSourcesJar() }
tasks.withType<JavaCompile>().configureEach { options.release.set(11) }

dependencies {
    api(project(":core"))
    // The app brings its own OkHttp and Retrofit; this module only needs them to compile.
    compileOnly(libs.okhttp)
    compileOnly(libs.retrofit)
    testImplementation(libs.okhttp)
    testImplementation(libs.okhttp.mockwebserver)
    testImplementation(libs.retrofit)
    testImplementation(testFixtures(project(":core")))
    testImplementation(libs.coroutines.core)
    testImplementation(libs.junit)
    testImplementation(kotlin("test"))
    testImplementation("org.json:json:20250517")
}

tasks.test {
    useJUnit()
    val defaultCore = rootProject.layout.projectDirectory.file("../../target/debug/sdk-test-core").asFile.absolutePath
    systemProperty("sdk.testcore", providers.gradleProperty("sdkTestCore").orElse(defaultCore).get())
}

publishing {
    publications { create<MavenPublication>("maven") { artifactId = "inspector-okhttp"; from(components["java"]) } }
}
