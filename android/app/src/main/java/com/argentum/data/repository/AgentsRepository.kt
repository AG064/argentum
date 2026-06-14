package com.argentum.data.repository

import com.argentum.data.model.Result
import com.argentum.viewmodel.Agent
import com.argentum.viewmodel.AgentStatus
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import java.util.concurrent.TimeUnit

class AgentsRepository(
    private val client: OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .build()
) {
    private val baseUrl = "https://api.argentum.example.com"

    /**
     * Fetches agents from the API.
     * Returns a Result sealed class for proper error handling.
     */
    suspend fun fetchAgents(): Result<List<Agent>> = withContext(Dispatchers.IO) {
        Result.runCatching {
            val request = Request.Builder()
                .url("$baseUrl/agents")
                .get()
                .build()

            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) {
                    throw Exception("API error: ${response.code} ${response.message}")
                }

                val body = response.body?.string()
                    ?: throw Exception("Empty response body")

                parseAgentsFromJson(body)
            }
        }
    }

    /**
     * Parses agents from JSON response.
     * In production, this would parse the actual API response.
     * For now, returns placeholder data.
     */
    private fun parseAgentsFromJson(json: String): List<Agent> {
        // Placeholder implementation
        // In production, parse JSON using kotlinx.serialization or Gson
        return try {
            val jsonArray = JSONArray(json)
            (0 until jsonArray.length()).map { index ->
                val obj = jsonArray.getJSONObject(index)
                Agent(
                    id = obj.getString("id"),
                    name = obj.getString("name"),
                    description = obj.optString("description", ""),
                    status = AgentStatus.valueOf(obj.optString("status", "IDLE").uppercase()),
                    icon = obj.optString("icon", "🤖")
                )
            }
        } catch (e: Exception) {
            // Return placeholder data if parsing fails
            getPlaceholderAgents()
        }
    }

    /**
     * Returns placeholder agents for demo/development purposes.
     */
    fun getPlaceholderAgents(): List<Agent> = listOf(
        Agent("1", "Argentum Core", "Main AI agent for general tasks", AgentStatus.ACTIVE, "🤖"),
        Agent("2", "Code Assistant", "Specialized in code review and suggestions", AgentStatus.IDLE, "💻"),
        Agent("3", "Research Agent", "Web search and information gathering", AgentStatus.BUSY, "🔍"),
        Agent("4", "Memory Keeper", "Manages persistent memory and context", AgentStatus.IDLE, "🧠")
    )
}
