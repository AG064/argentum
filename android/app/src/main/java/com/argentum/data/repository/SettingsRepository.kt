package com.argentum.data.repository

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import org.json.JSONArray
import org.json.JSONObject
import com.argentum.viewmodel.Conversation
import com.argentum.viewmodel.Message

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "settings")

class SettingsRepository(private val context: Context) {

    private object PreferencesKeys {
        val DARK_MODE = booleanPreferencesKey("dark_mode")
        val SELECTED_MODEL = stringPreferencesKey("selected_model")
        val SELECTED_PROVIDER = stringPreferencesKey("selected_provider")
        val API_ENDPOINT = stringPreferencesKey("api_endpoint")
        val API_KEY = stringPreferencesKey("api_key")
        val NOTIFICATIONS_ENABLED = booleanPreferencesKey("notifications_enabled")
        val ONBOARDING_COMPLETE = booleanPreferencesKey("onboarding_complete")
        val SYSTEM_PROMPT = stringPreferencesKey("system_prompt")
        val LOCAL_SERVER_URL = stringPreferencesKey("local_server_url")
        val CONVERSATIONS = stringPreferencesKey("conversations")
    }

    val darkModeFlow: Flow<Boolean> = context.dataStore.data
        .map { preferences -> preferences[PreferencesKeys.DARK_MODE] ?: true }

    val selectedModelFlow: Flow<String> = context.dataStore.data
        .map { preferences -> preferences[PreferencesKeys.SELECTED_MODEL] ?: "MiniMax-M2.7" }

    val apiEndpointFlow: Flow<String> = context.dataStore.data
        .map { preferences -> preferences[PreferencesKeys.API_ENDPOINT] ?: "https://api.minimax.io" }

    val apiKeyFlow: Flow<String> = context.dataStore.data
        .map { preferences -> preferences[PreferencesKeys.API_KEY] ?: "" }

    val notificationsEnabledFlow: Flow<Boolean> = context.dataStore.data
        .map { preferences -> preferences[PreferencesKeys.NOTIFICATIONS_ENABLED] ?: true }

    val selectedProviderFlow: Flow<String> = context.dataStore.data
        .map { preferences -> preferences[PreferencesKeys.SELECTED_PROVIDER] ?: "minimax" }

    val onboardingCompleteFlow: Flow<Boolean> = context.dataStore.data
        .map { preferences -> preferences[PreferencesKeys.ONBOARDING_COMPLETE] ?: false }

    val systemPromptFlow: Flow<String> = context.dataStore.data
        .map { preferences -> preferences[PreferencesKeys.SYSTEM_PROMPT] ?: "" }

    val localServerUrlFlow: Flow<String> = context.dataStore.data
        .map { preferences -> preferences[PreferencesKeys.LOCAL_SERVER_URL] ?: "http://127.0.0.1:8080/v1" }

    val conversationsFlow: Flow<List<Conversation>> = context.dataStore.data
        .map { preferences ->
            val json = preferences[PreferencesKeys.CONVERSATIONS] ?: "[]"
            parseConversations(json)
        }

    private fun parseConversations(json: String): List<Conversation> {
        return try {
            val jsonArray = JSONArray(json)
            (0 until jsonArray.length()).map { i ->
                val obj = jsonArray.getJSONObject(i)
                val messagesArray = obj.optJSONArray("messages") ?: JSONArray()
                val messages = (0 until messagesArray.length()).map { j ->
                    val msgObj = messagesArray.getJSONObject(j)
                    Message(
                        id = msgObj.optLong("id", System.currentTimeMillis()),
                        text = msgObj.optString("text", ""),
                        isUser = msgObj.optBoolean("isUser", true),
                        timestamp = msgObj.optLong("timestamp", System.currentTimeMillis()),
                        isError = msgObj.optBoolean("isError", false)
                    )
                }
                Conversation(
                    id = obj.optString("id", java.util.UUID.randomUUID().toString()),
                    title = obj.optString("title", "New Chat"),
                    messages = messages,
                    createdAt = obj.optLong("createdAt", System.currentTimeMillis()),
                    updatedAt = obj.optLong("updatedAt", System.currentTimeMillis())
                )
            }
        } catch (e: Exception) {
            emptyList()
        }
    }


    private fun conversationsToJson(conversations: List<Conversation>): String {
        val jsonArray = JSONArray()
        conversations.forEach { conv ->
            val obj = JSONObject().apply {
                put("id", conv.id)
                put("title", conv.title)
                put("createdAt", conv.createdAt)
                put("updatedAt", conv.updatedAt)
                val messagesArray = JSONArray()
                conv.messages.forEach { msg ->
                    messagesArray.put(JSONObject().apply {
                        put("id", msg.id)
                        put("text", msg.text)
                        put("isUser", msg.isUser)
                        put("timestamp", msg.timestamp)
                        put("isError", msg.isError)
                    })
                }
                put("messages", messagesArray)
            }
            jsonArray.put(obj)
        }
        return jsonArray.toString()
    }


    suspend fun saveConversations(conversations: List<Conversation>) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.CONVERSATIONS] = conversationsToJson(conversations)
        }
    }

    suspend fun setDarkMode(enabled: Boolean) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.DARK_MODE] = enabled
        }
    }

    suspend fun setSelectedModel(model: String) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.SELECTED_MODEL] = model
        }
    }

    suspend fun setSelectedProvider(provider: String) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.SELECTED_PROVIDER] = provider
        }
    }

    suspend fun setApiEndpoint(endpoint: String) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.API_ENDPOINT] = endpoint
        }
    }

    suspend fun setApiKey(apiKey: String) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.API_KEY] = apiKey
        }
    }

    suspend fun setNotificationsEnabled(enabled: Boolean) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.NOTIFICATIONS_ENABLED] = enabled
        }
    }

    suspend fun setOnboardingComplete(complete: Boolean) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.ONBOARDING_COMPLETE] = complete
        }
    }

    suspend fun setSystemPrompt(prompt: String) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.SYSTEM_PROMPT] = prompt
        }
    }

    suspend fun setLocalServerUrl(url: String) {
        context.dataStore.edit { preferences ->
            preferences[PreferencesKeys.LOCAL_SERVER_URL] = url
        }
    }
}