# Building and hosting the SDK

## What CI gives you

`.github/workflows/sdk.yml` builds and tests everything, then packages it:

| Archive | What it is | Who uses it |
|---|---|---|
| `proxy-inspector-android-maven-X.Y.Z.zip` | A complete Maven repository: the Gradle plugin, its marker, and the artifacts `inspector-android`, `-core`, `-okhttp`, `-ktor`, `-noop`, `-annotations` | Android developers |
| `inspector-sample-debug-X.Y.Z.apk` | The sample app | trying it on a phone |
| `proxy-inspector-ios-X.Y.Z.zip` | The Swift package sources | iOS developers, once the Apple-only code is verified (`sdk/ios/README.md`) |
| `sdk-dev-tools-linux-x86_64.tar.gz` | `sdk-test-core` and `fake-device` | people writing tests |
| `SHA256SUMS` | checksums | everyone |

When it runs: on every push or pull request that touches the SDK (archives are workflow artifacts, kept 90 days), and on a tag `sdk-vX.Y.Z`, which also opens a **draft** GitHub release with the archives attached. Nothing is public until you press Publish. To cut one:

```
git tag sdk-v0.1.0 && git push origin sdk-v0.1.0
```

The tag decides the version of every artifact (`-PsdkVersion`). Other runs use `0.0.0-ci.<run number>`.
The iOS job is allowed to fail for now: its Apple-only files have never been compiled, and that job is their first build. Rust and Android must be green for the release job to run.

## Ways to host it, easiest first

### 1. GitHub Release zip (works the moment you publish the draft)

Consumers download the Maven zip, unzip it, and point Gradle at the folder. No account, no token, but every developer does a download step.

```kotlin
// settings.gradle.kts
pluginManagement { repositories { maven(url = file("/path/to/proxy-inspector-maven")); google(); mavenCentral(); gradlePluginPortal() } }
dependencyResolutionManagement { repositories { maven(url = file("/path/to/proxy-inspector-maven")); google(); mavenCentral() } }
```

A team can also commit the unzipped repository into its own repository (a `libs/` folder) and use a relative path.

### 2. A Maven repository on GitHub Pages (public URL, no tokens for readers)

Run the workflow by hand (Actions, sdk, Run workflow) with **publish_pages** on. It pushes the repository to the `gh-pages` branch under `/maven`. Turn Pages on for that branch once (Settings, Pages). Then anyone can use:

```kotlin
pluginManagement { repositories { maven(url = "https://bharath914.github.io/proxy/maven/"); google(); mavenCentral(); gradlePluginPortal() } }
dependencyResolutionManagement { repositories { maven(url = "https://bharath914.github.io/proxy/maven/"); google(); mavenCentral() } }
```
```kotlin
plugins { id("com.android.application"); id("dev.proxy.inspector") version "0.1.0" }
```

It is public and permanent: only run it when you mean it. Each run adds the version it built; old versions stay (the job copies on top).
Limits: it is static hosting, so there is no authentication and no download statistics; Pages has soft limits on size (1 GB) and bandwidth.

### 3. GitHub Packages (private or org-internal)

Good for a company: set the repository's publishing target in `sdk/android/build.gradle.kts` to `https://maven.pkg.github.com/OWNER/REPO` with `GITHUB_TOKEN`. Readers need a personal access token with `read:packages` even for public packages, which is the main reason to prefer Pages for open source.

### 4. Maven Central and the Gradle Plugin Portal (the normal public route)

What developers expect: `mavenCentral()` plus `id("...") version "..."` with no extra repository lines. It needs a real name and group id first, a Sonatype Central account with a verified namespace, GPG signing, and a Plugin Portal account. The checklist is in `docs/sdk-publishing.md`. Do this when the name is final: coordinates cannot be renamed later.

### Swift

SwiftPM resolves a package from the root of a Git repository, and this package lives in `sdk/ios` inside a larger repository. So either:
- publish a small repository that contains only `sdk/ios` (mirror it on each tag, for example with `git subtree split --prefix sdk/ios`) and tag it; consumers use "Add Package" with that URL; or
- have a developer add the package from a local path (`.package(path: "../proxy/sdk/ios")`), which works today.

Do not do either for iOS until the Apple-only code has been built on a Mac.

## What I recommend

1. Now, for your own team: tag `sdk-v0.1.0`, publish the draft, and use option 1 or 2.
2. When you pick the final name: switch to option 4, keep option 2 for pre-releases.
3. Before anyone outside the team installs it: have the pairing design reviewed (`docs/security-review.md`) and run the device checklist (`docs/sdk/docs/e2e.md`).

## Things to know about the workflow

- The Android job needs the `platforms;android-37.1` platform because OkHttp 5.5 requires compileSdk 37. If GitHub's runner image lacks that package, the `sdkmanager` step installs it (it needs network).
- `:gradle-plugin:test` builds six Android apps and takes about six minutes.
- The workflow was written and checked for syntax here but **has not run on GitHub**. Expect to fix small things on the first run; each step mirrors a command that was run locally (`cargo test`, `./gradlew test`, `:gradle-plugin:test`, `publishAllToLocalRepo`, the sample build and `verifyNoInspectorInRelease`).
