plugins {
    alias(libs.plugins.kotlin.jvm)
    `maven-publish`
    `java-test-fixtures`
}

// Bytecode level 11 runs on every supported Android version; the JDK on the build machine can be newer.
kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11) } }
java { withSourcesJar() }
tasks.withType<JavaCompile>().configureEach { options.release.set(11) }

dependencies {
    api(project(":annotations"))
    testImplementation(libs.junit)
    testFixturesApi("org.json:json:20250517")
    testFixturesApi(project(":core"))
    testImplementation(kotlin("test"))
}

tasks.test {
    useJUnit()
    // Integration tests start the Rust ingest server; point them at a built binary.
    // `cargo build -p sdk --bins` at the repo root produces it.
    val defaultCore = rootProject.layout.projectDirectory.file("../../target/debug/sdk-test-core").asFile.absolutePath
    systemProperty("sdk.testcore", providers.gradleProperty("sdkTestCore").orElse(defaultCore).get())
    systemProperty("sdk.fixtures", rootProject.layout.projectDirectory.file("../../proto/sdk-fixtures.json").asFile.absolutePath)
}

// The test fixtures are not published.
components.withType<org.gradle.api.component.AdhocComponentWithVariants>().configureEach {
    withVariantsFromConfiguration(configurations["testFixturesApiElements"]) { skip() }
    withVariantsFromConfiguration(configurations["testFixturesRuntimeElements"]) { skip() }
}

publishing {
    publications { create<MavenPublication>("maven") { artifactId = "inspector-core"; from(components["java"]) } }
}
