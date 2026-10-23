# Spike S2: Android bytecode instrumentation of OkHttp

Status: **done.** AGP 9.4.1 and 8.13.1.

## Approach

`androidComponents.onVariants { variant.instrumentation.transformClassesWith(OkHttpClassVisitorFactory, InstrumentationScope.ALL) }`
with an ASM visitor (`AsmClassVisitorFactory`) that patches **the `OkHttpClient(Builder)` constructor**, not `Builder.build()`.

Why the constructor: every client passes through it, including `new OkHttpClient()` (which Retrofit uses for its
default client) and `newBuilder().build()`. Patching `build()` alone would miss those. The injected code is
`aload_1; invokestatic InspectorOkHttp.install(Builder)V` at the start of the constructor: a static call on a
parameter, which the verifier allows before `super()`.

`InspectorOkHttp.install` adds `InspectorInterceptor` only if the list has none, and wraps the builder's
`EventListener.Factory` only if it is not already ours, so it is safe to run any number of times and does not double a manual
`addInterceptor(InspectorInterceptor())`.

## Verified

| Check | How | Result |
|---|---|---|
| Clients created inside dependencies are covered | sample app: `:lib` module creates `OkHttpClient()`; scope `ALL` | The one patched constructor is in OkHttp's own classes, so every client in the app is covered. |
| Exactly one injected call | `PluginTest.theOkHttpConstructorIsInstrumentedExactlyOnce` runs `dexdump -d` on the APK | One `invoke-static InspectorOkHttp.install` in the whole APK, first instruction of `OkHttpClient.<init>(Builder)`. |
| Release is untouched | `PluginTest.debugGetsTheRealSdkAndReleaseOnly...` | Release dex has no `Ldev/nekoproxy/inspector/internal/` and no `InspectorOkHttp`. |
| Configuration cache | `PluginTest.anAppWithoutOkHttpStill...` | Second run reuses the entry. |
| Builds without OkHttp | same test, no OkHttp dependency | Builds. The factory only matches `okhttp3.OkHttpClient`. |

## Versions tested

AGP 9.4.1 with Gradle 9.8.1, AGP 8.13.1 with Gradle 8.14. OkHttp 5.5.0. The plugin is compiled against `gradle-api:8.5.2`
(the instrumentation API has not changed shape through 9.4).

## Surprises worth recording

- AGP 9.4 requires Gradle 9.6 or newer (read from the AGP release notes).
- AGP 9's built-in Kotlin is 2.2. Libraries compiled with a newer compiler cannot be read by it (metadata 2.4 vs 2.3 maximum). The SDK modules compile at
  language and API level 2.0 with `kotlin-stdlib:2.0.21` as their dependency, so the app's own version wins.
- OkHttp 5.5 needs `compileSdk` 37.
