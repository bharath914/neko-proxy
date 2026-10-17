plugins {
    alias(libs.plugins.kotlin.jvm) apply false
    alias(libs.plugins.android.library) apply false
}

// Every artifact shares one group and version. Change both in one place.
allprojects {
    group = "dev.proxy.inspector"
    // CI passes -PsdkVersion=X.Y.Z from the sdk-vX.Y.Z tag.
    version = providers.gradleProperty("sdkVersion").getOrElse("0.1.0")
}

// The SDK is read by the apps' own Kotlin compilers, which may be older than the one building it
// (AGP 9's built-in Kotlin is 2.2). Language and API level 2.0 write metadata every Kotlin 2.x can read.
subprojects {
    tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompilationTask<*>>().configureEach {
        compilerOptions {
            languageVersion.set(org.jetbrains.kotlin.gradle.dsl.KotlinVersion.KOTLIN_2_0)
            apiVersion.set(org.jetbrains.kotlin.gradle.dsl.KotlinVersion.KOTLIN_2_0)
        }
    }
}

// `./gradlew publishAllToLocalRepo` puts every artifact, and the Gradle plugin, in build/local-repo.
// The sample app and the plugin tests consume them from there, exactly as a real project would.
subprojects {
    plugins.withId("maven-publish") {
        extensions.configure<PublishingExtension> {
            repositories { maven { name = "local"; url = uri(rootProject.layout.buildDirectory.dir("local-repo")) } }
            // Metadata Maven Central and the Gradle plugin portal require. The URLs are placeholders until the
            // project has a name and a home (docs/sdk/docs/01-overview.md, open decisions). Nothing is published
            // without the owner's go-ahead: see docs/sdk-publishing.md.
            publications.withType<MavenPublication>().configureEach {
                pom {
                    name.set(artifactId)
                    description.set("In-app network inspector for debug builds: streams calls to the Proxy desktop app.")
                    url.set("https://example.invalid/proxy")
                    licenses {
                        license { name.set("MIT License"); url.set("https://opensource.org/license/mit") }
                        license { name.set("Apache License 2.0"); url.set("https://www.apache.org/licenses/LICENSE-2.0") }
                    }
                    developers { developer { id.set("proxy"); name.set("Proxy contributors") } }
                    scm { url.set("https://example.invalid/proxy.git") }
                }
            }
        }
    }
}

tasks.register("publishAllToLocalRepo") {
    group = "publishing"
    description = "Publishes every artifact and the Gradle plugin to build/local-repo."
    dependsOn(subprojects.map { "${it.path}:publishAllPublicationsToLocalRepository" })
}
