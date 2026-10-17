plugins {
    alias(libs.plugins.kotlin.jvm)
    `maven-publish`
}

// Bytecode level 11 runs on every supported Android version; the JDK on the build machine can be newer.
kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_11) } }
java { withSourcesJar() }
tasks.withType<JavaCompile>().configureEach { options.release.set(11) }

publishing {
    publications { create<MavenPublication>("maven") { artifactId = "inspector-annotations"; from(components["java"]) } }
}
