package com.argentum.data.api

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

data class ChatCompletionRequest(
    val model: String,
    val messages: List<ChatMessage>
)

data class ChatMessage(
    val role: String,
    val content: String
)

data class ChatCompletionResponse(
    val content: String,
    val model: String,
    val usage: Map<String, Int>
)

class MiniMaxApi {
    private val client = OkHttpClient.Builder()
        .connectTimeout(60, TimeUnit.SECONDS)
        .readTimeout(120, TimeUnit.SECONDS)
        .writeTimeout(60, TimeUnit.SECONDS)
        .build()

    suspend fun chatCompletion(
        apiKey: String,
        endpoint: String,
        request: ChatCompletionRequest
    ): Result<ChatCompletionResponse> = withContext(Dispatchers.IO) {
        try {
            val jsonBody = JSONObject().apply {
                put("model", request.model)
                put("messages", JSONArray().apply {
                    request.messages.forEach { msg ->
                        put(JSONObject().apply {
                            put("role", msg.role)
                            put("content", msg.content)
                        })
                    }
                })
            }

            val mediaType = "application/json".toMediaType()
            val requestBody = jsonBody.toString().toRequestBody(mediaType)

            val httpRequest = Request.Builder()
                .url("$endpoint/v1/chat/completions")
                .addHeader("Authorization", "Bearer $apiKey")
                .addHeader("Content-Type", "application/json")
                .post(requestBody)
                .build()

            val response = client.newCall(httpRequest).execute()
            val responseBody = response.body?.string()

            if (!response.isSuccessful) {
                return@withContext Result.failure(
                    Exception("API Error: ${response.code} - ${responseBody ?: "Unknown error"}")
                )
            }

            val jsonResponse = JSONObject(responseBody ?: "")
            val choices = jsonResponse.getJSONArray("choices")
            
            if (choices.length() == 0) {
                return@withContext Result.failure(Exception("No response from API"))
            }

            val firstChoice = choices.getJSONObject(0)
            val message = firstChoice.getJSONObject("message")
            val content = message.getString("content")
            val model = jsonResponse.optString("model", request.model)

            val usage = jsonResponse.optJSONObject("usage")?.let { usageObj ->
                mapOf(
                    "promptTokens" to usageObj.optInt("prompt_tokens", 0),
                    "completionTokens" to usageObj.optInt("completion_tokens", 0),
                    "totalTokens" to usageObj.optInt("total_tokens", 0)
                )
            } ?: emptyMap()

            Result.success(ChatCompletionResponse(content, model, usage))
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
}