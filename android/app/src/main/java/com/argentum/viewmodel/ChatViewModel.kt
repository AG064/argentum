package com.argentum.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.argentum.data.api.ChatMessage
import com.argentum.data.repository.ChatRepository
import com.argentum.data.repository.SettingsRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class Message(
    val id: Long = System.currentTimeMillis(),
    val text: String,
    val isUser: Boolean = true,
    val timestamp: Long = System.currentTimeMillis(),
    val isError: Boolean = false
)

data class ChatUiState(
    val messages: List<Message> = emptyList(),
    val inputText: String = "",
    val isLoading: Boolean = false,
    val error: String? = null
)

class ChatViewModel(
    private val chatRepository: ChatRepository,
    private val settingsRepository: SettingsRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(ChatUiState())
    val uiState: StateFlow<ChatUiState> = _uiState.asStateFlow()

    fun onInputChange(text: String) {
        _uiState.update { it.copy(inputText = text, error = null) }
    }

    fun sendMessage() {
        val text = _uiState.value.inputText.trim()
        if (text.isEmpty()) return

        _uiState.update { state ->
            state.copy(
                messages = state.messages + Message(text = text, isUser = true),
                inputText = "",
                isLoading = true,
                error = null
            )
        }

        viewModelScope.launch {
            try {
                val settings = settingsRepository.darkModeFlow.first() // just need to access repo
                val apiKey = settingsRepository.apiKeyFlow.first()
                val endpoint = settingsRepository.apiEndpointFlow.first()
                val model = settingsRepository.selectedModelFlow.first()

                if (apiKey.isBlank()) {
                    _uiState.update { state ->
                        state.copy(
                            messages = state.messages + Message(
                                text = "Please configure your API key in Settings to use the chat.",
                                isUser = false,
                                isError = true
                            ),
                            isLoading = false,
                            error = "API key not configured"
                        )
                    }
                    return@launch
                }

                val chatMessages = _uiState.value.messages.map { msg ->
                    ChatMessage(
                        role = if (msg.isUser) "user" else "assistant",
                        content = msg.text
                    )
                }

                val result = chatRepository.sendMessage(
                    apiKey = apiKey,
                    endpoint = endpoint,
                    model = model,
                    messages = chatMessages
                )

                result.fold(
                    onSuccess = { response ->
                        _uiState.update { state ->
                            state.copy(
                                messages = state.messages + Message(
                                    text = response,
                                    isUser = false
                                ),
                                isLoading = false
                            )
                        }
                    },
                    onFailure = { error ->
                        _uiState.update { state ->
                            state.copy(
                                messages = state.messages + Message(
                                    text = "Error: ${error.message ?: "Failed to get response"}",
                                    isUser = false,
                                    isError = true
                                ),
                                isLoading = false,
                                error = error.message
                            )
                        }
                    }
                )
            } catch (e: Exception) {
                _uiState.update { state ->
                    state.copy(
                        messages = state.messages + Message(
                            text = "Error: ${e.message ?: "Unknown error occurred"}",
                            isUser = false,
                            isError = true
                        ),
                        isLoading = false,
                        error = e.message
                    )
                }
            }
        }
    }

    fun clearChat() {
        _uiState.update { ChatUiState() }
    }
}