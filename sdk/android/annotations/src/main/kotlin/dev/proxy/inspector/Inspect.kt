package dev.proxy.inspector

/**
 * Names a Retrofit call for the desktop inspector. Put it on an interface method, or on the
 * interface to apply to every method. Ships in every build type and does nothing at runtime on
 * its own.
 *
 * ```
 * @Inspect(name = "Apply coupon", owner = "payments")
 * @POST("cart/{id}/coupon")
 * suspend fun applyCoupon(@Path("id") id: String, @Body body: Coupon): Cart
 * ```
 */
@Retention(AnnotationRetention.RUNTIME)
@Target(AnnotationTarget.FUNCTION, AnnotationTarget.CLASS)
annotation class Inspect(
    val name: String = "",
    val owner: String = "",
    val tag: String = "",
)
