package com.argentum.data.repository

import com.argentum.data.api.ChatCompletionRequest
import com.argentum.data.api.ChatMessage
import com.argentum.data.api.MiniMaxApi

class ChatRepository(
    private val api: MiniMaxApi = MiniMaxApi()
) {
    suspend fun sendMessage(
        apiKey: String,
        endpoint: String,
        model: String,
        messages: List<ChatMessage>
    ): Result<String> {
        val request = ChatCompletionRequest(
            model = model,
            messages = messages
        )

        return api.chatCompletion(apiKey, endpoint, request).map { response ->
            response.content
        }
    }
}