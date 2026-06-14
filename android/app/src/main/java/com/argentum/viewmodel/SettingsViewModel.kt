package com.argentum.viewmodel

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

data class SettingsUiState(
    val isDarkMode: Boolean = true,
    val selectedModel: String = "MiniMax-M2.7",
    val availableModels: List<String> = listOf(
        "MiniMax-M2.7",
        "MiniMax-M2",
        "GPT-4o",
        "Claude-3.5"
    ),
    val apiEndpoint: String = "https://api.minimax.io",
    val notificationsEnabled: Boolean = true
)

class SettingsViewModel : ViewModel() {
    private val _uiState = MutableStateFlow(SettingsUiState())
    val uiState: StateFlow<SettingsUiState> = _uiState.asStateFlow()

    fun toggleDarkMode() {
        _uiState.update { it.copy(isDarkMode = !it.isDarkMode) }
    }

    fun selectModel(model: String) {
        _uiState.update { it.copy(selectedModel = model) }
    }

    fun updateApiEndpoint(endpoint: String) {
        _uiState.update { it.copy(apiEndpoint = endpoint) }
    }

    fun toggleNotifications() {
        _uiState.update { it.copy(notificationsEnabled = !it.notificationsEnabled) }
    }
}
