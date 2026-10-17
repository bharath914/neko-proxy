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

// `./gradlew :okhttp:bench` runs the benchmark scenarios on the JVM.
tasks.register<JavaExec>("bench") {
    group = "verification"
    classpath = sourceSets["test"].runtimeClasspath
    mainClass.set("dev.proxy.inspector.BenchKt")
    systemProperty("repo.root", rootProject.layout.projectDirectory.dir("../..").asFile.absolutePath)
    systemProperty("bench.out", rootProject.layout.buildDirectory.file("sdk-bench-android.md").get().asFile.absolutePath)
    val defaultCore = rootProject.layout.projectDirectory.file("../../target/debug/sdk-test-core").asFile.absolutePath
    systemProperty("sdk.testcore", providers.gradleProperty("sdkTestCore").orElse(defaultCore).get())
}

// `./gradlew :okhttp:e2e -Pargs="8897 http://127.0.0.1:18080/ 8899"`, used by sdk/scripts/e2e-jvm.sh.
tasks.register<JavaExec>("e2e") {
    group = "verification"
    classpath = sourceSets["test"].runtimeClasspath
    mainClass.set("dev.proxy.inspector.E2eKt")
    args = (providers.gradleProperty("args").orNull ?: "8897 http://127.0.0.1:18080/ 8899").split(" ")
}
