package com.argentum.viewmodel

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.argentum.data.repository.SettingsRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class OnboardingUiState(
    val isDarkMode: Boolean = true,
    val selectedProvider: String = "minimax",
    val apiKey: String = "",
    val endpoint: String = "http://127.0.0.1:8080/v1",
    val model: String = "MiniMax-M2.7",
    val isComplete: Boolean = false
)

class OnboardingViewModel(
    private val settingsRepository: SettingsRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(OnboardingUiState())
    val uiState: StateFlow<OnboardingUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            _uiState.update { state ->
                state.copy(
                    isDarkMode = settingsRepository.darkModeFlow.first(),
                    selectedProvider = settingsRepository.selectedProviderFlow.first().ifEmpty { "minimax" },
                    apiKey = settingsRepository.apiKeyFlow.first(),
                    endpoint = settingsRepository.apiEndpointFlow.first().ifEmpty { "http://127.0.0.1:8080/v1" },
                    model = settingsRepository.selectedModelFlow.first().ifEmpty { "MiniMax-M2.7" }
                )
            }
        }
    }

    fun toggleDarkMode() {
        viewModelScope.launch {
            val newValue = !_uiState.value.isDarkMode
            settingsRepository.setDarkMode(newValue)
            _uiState.update { it.copy(isDarkMode = newValue) }
        }
    }

    fun selectProvider(provider: String) {
        viewModelScope.launch {
            settingsRepository.setSelectedProvider(provider)
            
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
            
            settingsRepository.setSelectedModel(model)
            settingsRepository.setApiEndpoint(endpoint)
            
            _uiState.update { it.copy(selectedProvider = provider, model = model, endpoint = endpoint) }
        }
    }

    fun updateApiKey(apiKey: String) {
        viewModelScope.launch {
            settingsRepository.setApiKey(apiKey)
            _uiState.update { it.copy(apiKey = apiKey) }
        }
    }

    fun updateEndpoint(endpoint: String) {
        viewModelScope.launch {
            settingsRepository.setApiEndpoint(endpoint)
            _uiState.update { it.copy(endpoint = endpoint) }
        }
    }

    fun updateModel(model: String) {
        viewModelScope.launch {
            settingsRepository.setSelectedModel(model)
            _uiState.update { it.copy(model = model) }
        }
    }

    fun completeOnboarding() {
        viewModelScope.launch {
            settingsRepository.setOnboardingComplete(true)
            _uiState.update { it.copy(isComplete = true) }
        }
    }
}

class OnboardingViewModelFactory(
    private val context: Context
) : androidx.lifecycle.ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T {
        return OnboardingViewModel(SettingsRepository(context)) as T
    }
}
