# The Gradle plugin's bytecode instrumentation calls this by name.
-keep class dev.nekoproxy.inspector.InspectorOkHttp { *; }
# Optional integrations: present only if the app uses them.
-dontwarn okhttp3.**
-dontwarn retrofit2.**
-dontwarn io.ktor.**
-dontwarn androidx.fragment.**
