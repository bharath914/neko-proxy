package dev.proxy.inspector.android

import android.app.Activity
import android.app.Application
import android.os.Bundle
import dev.proxy.inspector.Inspector

/** The resumed activity's class name is the screen. Fragments, when the app has them, refine it. */
internal class ScreenTracker : Application.ActivityLifecycleCallbacks {
    private val fragments = try { Class.forName("androidx.fragment.app.FragmentActivity"); true } catch (e: Throwable) { false }

    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {
        if (fragments) runCatching { FragmentScreens.register(activity) }
    }

    override fun onActivityResumed(activity: Activity) { Inspector.setScreen(activity.javaClass.simpleName) }
    override fun onActivityStarted(activity: Activity) {}
    override fun onActivityPaused(activity: Activity) {}
    override fun onActivityStopped(activity: Activity) {}
    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
    override fun onActivityDestroyed(activity: Activity) {}
}

/** Loaded only when androidx.fragment is on the classpath. */
private object FragmentScreens {
    fun register(activity: Activity) {
        val fa = activity as? androidx.fragment.app.FragmentActivity ?: return
        fa.supportFragmentManager.registerFragmentLifecycleCallbacks(object : androidx.fragment.app.FragmentManager.FragmentLifecycleCallbacks() {
            override fun onFragmentResumed(fm: androidx.fragment.app.FragmentManager, f: androidx.fragment.app.Fragment) {
                // Skip containers and dialogs' hosts that have child fragments; the leaf is the screen.
                if (f.childFragmentManager.fragments.isEmpty() && f.isVisible) Inspector.setScreen("${activity.javaClass.simpleName}/${f.javaClass.simpleName}")
            }
        }, true)
    }
}
