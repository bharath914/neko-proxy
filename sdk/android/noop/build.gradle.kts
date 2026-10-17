plugins {
    alias(libs.plugins.kotlin.jvm)
    `maven-publish`
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11) } }
java { withSourcesJar() }
tasks.withType<JavaCompile>().configureEach { options.release.set(11) }

dependencies {
    api("org.jetbrains.kotlin:kotlin-stdlib:2.0.21")
    // Same public API as the real artifacts, no logic. @Inspect comes from the annotations artifact.
    api(project(":annotations"))
    compileOnly(libs.okhttp)
    compileOnly(libs.ktor.client.core.oldest)
    testImplementation(libs.okhttp)
    testImplementation(libs.ktor.client.core)
    testImplementation(libs.coroutines.core)
    testImplementation(kotlin("reflect"))
    testImplementation(libs.junit)
    testImplementation(kotlin("test"))
}

listOf(":core", ":okhttp", ":ktor").forEach { evaluationDependsOn(it) }
val realJars = listOf(":core", ":okhttp", ":ktor").map { project(it).tasks.named("jar") }

tasks.test {
    useJUnit()
    dependsOn(realJars, tasks.named("jar"))
    // The parity test compares the real artifacts' public API with this module's.
    systemProperty("jars.real", realJars.joinToString(File.pathSeparator) { it.get().outputs.files.singleFile.absolutePath })
    systemProperty("jars.noop", tasks.named("jar").get().outputs.files.singleFile.absolutePath)
}

publishing {
    publications { create<MavenPublication>("maven") { artifactId = "inspector-noop"; from(components["java"]) } }
}
