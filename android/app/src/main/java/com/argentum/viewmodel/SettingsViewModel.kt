package com.argentum.viewmodel

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import com.argentum.data.repository.SettingsRepository
import com.argentum.data.update.UpdateChecker
import com.argentum.data.update.UpdateInstaller
import com.argentum.data.update.UpdateResult
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * In-app updater state machine. Owned by the SettingsViewModel; the UI maps
 * this to buttons and dialogs.
 *
 *   Idle          - nothing happening, "Check for updates" button is enabled
 *   Checking      - network call in flight
 *   UpToDate      - remote version <= current
 *   Available     - remote version > current, ready to install
 *   Downloading   - system DownloadManager is fetching the APK
 *   ReadyToInstall- APK downloaded, content URI ready to hand to the installer
 *   Error         - network / parse / download failure; UI shows message + retry
 */
sealed interface UpdateState {
    data object Idle : UpdateState
    data object Checking : UpdateState
    data class UpToDate(val currentVersion: String) : UpdateState
    data class Available(
        val version: String,
        val releaseNotes: String,
        val apkUrl: String,
        val apkSize: Long,
        val releasePageUrl: String,
    ) : UpdateState
    data class Downloading(val version: String) : UpdateState
    data class ReadyToInstall(val version: String, val apkUri: android.net.Uri) : UpdateState
    data class Error(val message: String) : UpdateState
}

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
    val localServerUrl: String = "http://127.0.0.1:8080/v1",
    val updateState: UpdateState = UpdateState.Idle,
)

private data class ProviderSettings(
    val provider: String,
    val model: String,
    val endpoint: String,
    val apiKey: String,
)

private data class LocalSettings(
    val darkMode: Boolean,
    val notificationsEnabled: Boolean,
    val systemPrompt: String,
    val localServerUrl: String,
)

class SettingsViewModel(
    private val repository: SettingsRepository,
    private val updateChecker: UpdateChecker = UpdateChecker(),
    private val updateInstaller: UpdateInstaller? = null,
) : ViewModel() {

    private val _uiState = MutableStateFlow(SettingsUiState())
    val uiState: StateFlow<SettingsUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            val providerSettings = combine(
                repository.selectedProviderFlow,
                repository.selectedModelFlow,
                repository.apiEndpointFlow,
                repository.apiKeyFlow,
            ) { provider, model, endpoint, apiKey ->
                ProviderSettings(provider, model, endpoint, apiKey)
            }
            val localSettings = combine(
                repository.darkModeFlow,
                repository.notificationsEnabledFlow,
                repository.systemPromptFlow,
                repository.localServerUrlFlow
            ) { darkMode, notifications, systemPrompt, localServer ->
                LocalSettings(darkMode, notifications, systemPrompt, localServer)
            }
            combine(providerSettings, localSettings) { provider, local ->
                SettingsUiState(
                    isDarkMode = local.darkMode,
                    selectedProvider = provider.provider,
                    selectedModel = provider.model,
                    apiEndpoint = provider.endpoint,
                    apiKey = provider.apiKey,
                    notificationsEnabled = local.notificationsEnabled,
                    systemPrompt = local.systemPrompt,
                    localServerUrl = local.localServerUrl,
                    updateState = _uiState.value.updateState,
                )
            }.collect { state ->
                _uiState.update { state }
            }
        }
    }

    fun checkForUpdates() {
        if (_uiState.value.updateState is UpdateState.Checking) return
        _uiState.update { it.copy(updateState = UpdateState.Checking) }
        viewModelScope.launch {
            val result = updateChecker.check()
            val next = when (result) {
                is UpdateResult.UpdateAvailable -> UpdateState.Available(
                    version = result.version,
                    releaseNotes = result.releaseNotes,
                    apkUrl = result.apkUrl,
                    apkSize = result.apkSize,
                    releasePageUrl = result.releasePageUrl,
                )
                is UpdateResult.UpToDate -> UpdateState.UpToDate(result.currentVersion)
                is UpdateResult.Error -> UpdateState.Error(result.message)
            }
            _uiState.update { it.copy(updateState = next) }
        }
    }

    fun downloadUpdate() {
        val current = _uiState.value.updateState
        if (current !is UpdateState.Available) return
        val installer = updateInstaller
            ?: return _uiState.update {
                it.copy(updateState = UpdateState.Error("Installer unavailable"))
            }
        _uiState.update { it.copy(updateState = UpdateState.Downloading(current.version)) }
        viewModelScope.launch {
            try {
                val suggested = "argentum-v${current.version}-android.apk"
                val uri = installer.downloadAndResolve(current.apkUrl, suggested)
                _uiState.update {
                    it.copy(
                        updateState = UpdateState.ReadyToInstall(current.version, uri)
                    )
                }
            } catch (e: Exception) {
                _uiState.update {
                    it.copy(
                        updateState = UpdateState.Error(
                            e.message ?: "Download failed"
                        )
                    )
                }
            }
        }
    }

    fun dismissUpdateState() {
        _uiState.update { it.copy(updateState = UpdateState.Idle) }
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
                return SettingsViewModel(
                    repository = SettingsRepository(context),
                    updateInstaller = UpdateInstaller(context.applicationContext),
                ) as T
            }
            throw IllegalArgumentException("Unknown ViewModel class")
        }
    }
}
