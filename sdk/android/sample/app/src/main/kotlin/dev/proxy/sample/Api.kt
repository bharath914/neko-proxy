package dev.proxy.sample

import dev.proxy.inspector.Inspect
import okhttp3.ResponseBody
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path

interface ShopApi {
    @Inspect(name = "List items", owner = "catalog")
    @GET("json")
    suspend fun items(): ResponseBody

    @Inspect(name = "Echo order", owner = "payments", tag = "checkout")
    @POST("echo")
    suspend fun order(@Body body: okhttp3.RequestBody): ResponseBody

    @GET("status/{code}")
    suspend fun status(@Path("code") code: Int): ResponseBody
}
