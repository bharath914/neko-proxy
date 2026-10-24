package dev.nekoproxy.inspector.gradle

import com.android.build.api.instrumentation.AsmClassVisitorFactory
import com.android.build.api.instrumentation.ClassContext
import com.android.build.api.instrumentation.ClassData
import com.android.build.api.instrumentation.InstrumentationParameters
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes

/**
 * Patches the `OkHttpClient(Builder)` constructor, the one place every client passes through
 * (`Builder.build()`, `OkHttpClient()`, `newBuilder().build()`), including clients created inside
 * dependencies. It calls `InspectorOkHttp.install(builder)` first, which adds the interceptor and
 * the timing listener unless they are already there, so a manual `addInterceptor(InspectorInterceptor())`
 * is never doubled.
 */
abstract class OkHttpClassVisitorFactory : AsmClassVisitorFactory<InstrumentationParameters.None> {
    override fun isInstrumentable(classData: ClassData): Boolean = classData.className == CLIENT

    override fun createClassVisitor(classContext: ClassContext, nextClassVisitor: ClassVisitor): ClassVisitor =
        object : ClassVisitor(Opcodes.ASM9, nextClassVisitor) {
            override fun visitMethod(access: Int, name: String, descriptor: String, signature: String?, exceptions: Array<out String>?): MethodVisitor {
                val mv = super.visitMethod(access, name, descriptor, signature, exceptions)
                if (name != "<init>" || descriptor != BUILDER_CTOR) return mv
                return object : MethodVisitor(Opcodes.ASM9, mv) {
                    override fun visitCode() {
                        super.visitCode()
                        // InspectorOkHttp.install(builder): a static call on a parameter, allowed before super().
                        visitVarInsn(Opcodes.ALOAD, 1)
                        visitMethodInsn(Opcodes.INVOKESTATIC, INSTALLER, "install", "(Lokhttp3/OkHttpClient\$Builder;)V", false)
                    }
                }
            }
        }

    companion object {
        const val CLIENT = "okhttp3.OkHttpClient"
        const val BUILDER_CTOR = "(Lokhttp3/OkHttpClient\$Builder;)V"
        const val INSTALLER = "dev/nekoproxy/inspector/InspectorOkHttp"
    }
}
