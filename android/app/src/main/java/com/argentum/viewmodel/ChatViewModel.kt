package com.argentum.viewmodel

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

data class Message(
    val id: Long = System.currentTimeMillis(),
    val text: String,
    val isUser: Boolean = true,
    val timestamp: Long = System.currentTimeMillis()
)

data class ChatUiState(
    val messages: List<Message> = emptyList(),
    val inputText: String = "",
    val isLoading: Boolean = false
)

class ChatViewModel : ViewModel() {
    private val _uiState = MutableStateFlow(ChatUiState())
    val uiState: StateFlow<ChatUiState> = _uiState.asStateFlow()

    fun onInputChange(text: String) {
        _uiState.update { it.copy(inputText = text) }
    }

    fun sendMessage() {
        val text = _uiState.value.inputText.trim()
        if (text.isEmpty()) return

        _uiState.update { state ->
            state.copy(
                messages = state.messages + Message(text = text, isUser = true),
                inputText = "",
                isLoading = true
            )
        }

        // Simulate AI response
        _uiState.update { state ->
            state.copy(
                messages = state.messages + Message(
                    text = "Argentum AI: Message received. This is a demo response.",
                    isUser = false
                ),
                isLoading = false
            )
        }
    }
}
