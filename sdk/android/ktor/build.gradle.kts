plugins {
    alias(libs.plugins.kotlin.jvm)
    `maven-publish`
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11) } }
java { withSourcesJar() }
tasks.withType<JavaCompile>().configureEach { options.release.set(11) }

dependencies {
    api(project(":core"))
    compileOnly(libs.ktor.client.core)
    compileOnly(libs.coroutines.core)
    testImplementation(libs.ktor.client.core)
    testImplementation(libs.ktor.client.mock)
    testImplementation(libs.ktor.client.cio)
    testImplementation(libs.ktor.client.okhttp)
    testImplementation(libs.okhttp)
    testImplementation(project(":okhttp"))
    testImplementation(libs.okhttp.mockwebserver)
    testImplementation(testFixtures(project(":core")))
    testImplementation(libs.coroutines.core)
    testImplementation(libs.junit)
    testImplementation(kotlin("test"))
}

tasks.test {
    useJUnit()
    val defaultCore = rootProject.layout.projectDirectory.file("../../target/debug/sdk-test-core").asFile.absolutePath
    systemProperty("sdk.testcore", providers.gradleProperty("sdkTestCore").orElse(defaultCore).get())
}

publishing {
    publications { create<MavenPublication>("maven") { artifactId = "inspector-ktor"; from(components["java"]) } }
}
