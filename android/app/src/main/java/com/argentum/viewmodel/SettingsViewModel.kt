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
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class SettingsUiState(
    val isDarkMode: Boolean = true,
    val selectedModel: String = "MiniMax-M2.7",
    val availableModels: List<String> = listOf(
        "MiniMax-M2.7",
        "MiniMax-M3",
        "GPT-4o",
        "Claude-3.5"
    ),
    val apiEndpoint: String = "https://api.minimax.io",
    val apiKey: String = "",
    val notificationsEnabled: Boolean = true
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
                repository.selectedModelFlow,
                repository.apiEndpointFlow,
                repository.apiKeyFlow,
                repository.notificationsEnabledFlow
            ) { darkMode, model, endpoint, apiKey, notifications ->
                SettingsUiState(
                    isDarkMode = darkMode,
                    selectedModel = model,
                    apiEndpoint = endpoint,
                    apiKey = apiKey,
                    notificationsEnabled = notifications
                )
            }.collect { state ->
                _uiState.update { state }
            }
        }
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