plugins {
    alias(libs.plugins.kotlin.jvm)
    `maven-publish`
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11) } }
java { withSourcesJar() }
tasks.withType<JavaCompile>().configureEach { options.release.set(11) }

dependencies {
    api("org.jetbrains.kotlin:kotlin-stdlib:2.0.21")
    api(project(":core"))
    compileOnly(libs.ktor.client.core.oldest)
    compileOnly(libs.coroutines.core)
    testImplementation("io.ktor:ktor-client-core:${libs.versions.ktorOldest.get()}")
    testImplementation("io.ktor:ktor-client-mock:${libs.versions.ktorOldest.get()}")
    testImplementation("io.ktor:ktor-client-cio:${libs.versions.ktorOldest.get()}")
    testImplementation("io.ktor:ktor-client-okhttp:${libs.versions.ktorOldest.get()}")
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

// Tests compile and run on the oldest supported Ktor (libs.versions.toml). The same compiled classes then run on the
// newest Ktor: that is the direction that matters, an app on a newer Ktor than the SDK was built with.
// `./gradlew :ktor:testNewestKtor`
val newestKtor = configurations.create("newestKtor") {
    extendsFrom(configurations.testRuntimeClasspath.get())
    resolutionStrategy.eachDependency { if (requested.group == "io.ktor") useVersion(libs.versions.ktor.get()) }
}
tasks.register<Test>("testNewestKtor") {
    group = "verification"
    useJUnit()
    testClassesDirs = sourceSets.test.get().output.classesDirs
    classpath = sourceSets.test.get().output + sourceSets.main.get().output + newestKtor
    systemProperty("sdk.testcore", providers.gradleProperty("sdkTestCore").orElse(rootProject.layout.projectDirectory.file("../../target/debug/sdk-test-core").asFile.absolutePath).get())
}
tasks.check { dependsOn("testNewestKtor") }
