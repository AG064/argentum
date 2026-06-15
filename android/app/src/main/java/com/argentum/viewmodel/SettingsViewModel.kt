package com.argentum.viewmodel

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import com.argentum.data.repository.SettingsRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class SettingsUiState(
    val isDarkMode: Boolean = true,
    val selectedProvider: String = "minimax",
    val selectedModel: String = "MiniMax-M2.7",
    val availableModels: List<String> = listOf(
        "MiniMax-M2.7",
        "MiniMax-M3",
        "GPT-4o",
        "Claude-3.5"
    ),
    val apiEndpoint: String = "https://api.minimax.io",
    val apiKey: String = "",
    val notificationsEnabled: Boolean = true,
    val systemPrompt: String = "",
    val localServerUrl: String = "http://127.0.0.1:8080/v1"
)

class SettingsViewModel(
    private val repository: SettingsRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(SettingsUiState())
    val uiState: StateFlow<SettingsUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            combine(
                repository.darkModeFlow,
                repository.selectedProviderFlow,
                repository.selectedModelFlow,
                repository.apiEndpointFlow,
                repository.apiKeyFlow,
                repository.notificationsEnabledFlow,
                repository.systemPromptFlow,
                repository.localServerUrlFlow
            ) { darkMode, provider, model, endpoint, apiKey, notifications, systemPrompt, localServer ->
                SettingsUiState(
                    isDarkMode = darkMode,
                    selectedProvider = provider,
                    selectedModel = model,
                    apiEndpoint = endpoint,
                    apiKey = apiKey,
                    notificationsEnabled = notifications,
                    systemPrompt = systemPrompt,
                    localServerUrl = localServer
                )
            }.collect { state ->
                _uiState.update { state }
            }
        }
    }

    suspend fun isOnboardingComplete(): Boolean {
        return repository.onboardingCompleteFlow.first()
    }

    fun toggleDarkMode() {
        viewModelScope.launch {
            repository.setDarkMode(!_uiState.value.isDarkMode)
        }
    }

    fun selectModel(model: String) {
        viewModelScope.launch {
            repository.setSelectedModel(model)
        }
    }

    fun updateApiEndpoint(endpoint: String) {
        viewModelScope.launch {
            repository.setApiEndpoint(endpoint)
        }
    }

    fun updateApiKey(apiKey: String) {
        viewModelScope.launch {
            repository.setApiKey(apiKey)
        }
    }

    fun toggleNotifications() {
        viewModelScope.launch {
            repository.setNotificationsEnabled(!_uiState.value.notificationsEnabled)
        }
    }

    fun updateSystemPrompt(prompt: String) {
        viewModelScope.launch {
            repository.setSystemPrompt(prompt)
        }
    }

    fun updateLocalServerUrl(url: String) {
        viewModelScope.launch {
            repository.setLocalServerUrl(url)
        }
    }

    fun selectProvider(provider: String) {
        viewModelScope.launch {
            repository.setSelectedProvider(provider)
            val model = when (provider) {
                "minimax" -> "MiniMax-M2.7"
                "openai" -> "gpt-4o-mini"
                "local" -> "local-model"
                else -> "MiniMax-M2.7"
            }
            val endpoint = when (provider) {
                "minimax" -> "https://api.minimax.io"
                "openai" -> "https://api.openai.com/v1"
                "local" -> "http://127.0.0.1:8080/v1"
                else -> "https://api.minimax.io"
            }
            repository.setSelectedModel(model)
            repository.setApiEndpoint(endpoint)
        }
    }

    class Factory(private val context: Context) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : ViewModel> create(modelClass: Class<T>): T {
            if (modelClass.isAssignableFrom(SettingsViewModel::class.java)) {
                return SettingsViewModel(SettingsRepository(context)) as T
            }
            throw IllegalArgumentException("Unknown ViewModel class")
        }
    }
}