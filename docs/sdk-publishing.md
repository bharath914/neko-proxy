# Publishing the SDK artifacts

**Nothing here has been run against Maven Central, the Gradle plugin portal or a Swift package tag. Do not publish without the owner's go-ahead.**
The final product name, group id, plugin id and Swift package name are open decisions (`docs/sdk/docs/01-overview.md`); the values in the build (`dev.nekoproxy.inspector`) are placeholders.

## Android, what exists

- Every module has a `maven-publish` publication with POM metadata (license, developers, scm; the URLs are placeholders).
- `./gradlew publishAllToLocalRepo` writes everything, plus the Gradle plugin and its marker artifact, to `sdk/android/build/local-repo`.
  The sample app and `PluginTest` consume that repository exactly as a real project would. This is how "a clean project can consume the locally published artifacts" is verified.
- Artifacts: `inspector-annotations`, `inspector-core`, `inspector-okhttp`, `inspector-ktor`, `inspector-android` (AAR), `inspector-noop`, and the plugin `dev.nekoproxy.inspector` (with `gradle-plugin` as its implementation artifact).

## Android, what is still needed before a real release

1. Decide the name, group id and plugin id. Search-and-replace `dev.nekoproxy.inspector` in `sdk/android` (group, package names, manifest meta-data, plugin id, `InspectorInitializer` name in the manifest).
2. Real POM URLs.
3. Maven Central: a Sonatype Central account and verified namespace, a GPG key, `signing` and the Central publishing plugin configured in the root build, sources and Javadoc jars (sources exist; add a Dokka or empty Javadoc jar).
4. Gradle plugin portal: an account and API keys, `com.gradle.plugin-publish` applied to `:gradle-plugin`.
5. Bump `version` in the root `build.gradle.kts` and tag.
6. Run `./gradlew test :gradle-plugin:test` and the emulator checks in `docs/sdk/docs/e2e.md`. Get the pairing design reviewed (`docs/security-review.md`).

## iOS

Not built (needs macOS and Xcode). When it is: tag the package repository with a semantic version; SwiftPM resolves tags. The package directory is `sdk/ios`.
