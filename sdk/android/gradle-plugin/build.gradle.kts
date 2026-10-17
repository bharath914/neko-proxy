plugins {
    alias(libs.plugins.kotlin.jvm)
    `java-gradle-plugin`
    `maven-publish`
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
java { withSourcesJar() }
tasks.withType<JavaCompile>().configureEach { options.release.set(17) }

dependencies {
    compileOnly("org.jetbrains.kotlin:kotlin-stdlib:2.0.21")
    testImplementation("org.jetbrains.kotlin:kotlin-stdlib:2.0.21")
    // Compiled against an older, stable AGP API on purpose: the instrumentation and variant APIs used
    // here have not changed shape since, and the tests run it on AGP 8.x and 9.x.
    compileOnly("com.android.tools.build:gradle-api:8.5.2")
    compileOnly("org.ow2.asm:asm:9.7.1")
    testImplementation(gradleTestKit())
    testImplementation(libs.junit)
    testImplementation(kotlin("test"))
}

gradlePlugin {
    plugins {
        create("inspector") {
            // Placeholder id: the final name is an open decision (docs/sdk/docs/01-overview.md).
            id = "dev.proxy.inspector"
            implementationClass = "dev.proxy.inspector.gradle.InspectorGradlePlugin"
            displayName = "Proxy Inspector"
            description = "Streams a debug build's network calls to the Proxy desktop app. Release builds get a no-op."
        }
    }
}

tasks.processResources {
    // The plugin adds artifacts of its own version.
    val v = project.version.toString()
    inputs.property("version", v)
    filesMatching("inspector-plugin.properties") { expand("version" to v) }
}

tasks.test {
    useJUnit()
    // The tests consume what `publishAllToLocalRepo` produces.
    dependsOn(":publishAllToLocalRepo")
    // Tests build real Android projects against artifacts published to this repository.
    systemProperty("inspector.repo", rootProject.layout.buildDirectory.dir("local-repo").get().asFile.absolutePath)
    systemProperty("inspector.version", project.version.toString())
    systemProperty("android.home", System.getenv("ANDROID_HOME") ?: "${System.getProperty("user.home")}/Android/Sdk")
    maxParallelForks = 1
}
