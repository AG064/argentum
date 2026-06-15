package com.argentum.viewmodel

import android.content.Context
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

data class Conversation(
    val id: String,
    val title: String,
    val messages: List<Message>,
    val createdAt: Long = System.currentTimeMillis(),
    val updatedAt: Long = System.currentTimeMillis()
)

data class ChatUiState(
    val messages: List<Message> = emptyList(),
    val inputText: String = "",
    val isLoading: Boolean = false,
    val error: String? = null,
    val thinking: String = "",
    val conversations: List<Conversation> = emptyList(),
    val currentConversationId: String? = null
)

class ChatViewModel(
    private val chatRepository: ChatRepository,
    private val settingsRepository: SettingsRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(ChatUiState())
    val uiState: StateFlow<ChatUiState> = _uiState.asStateFlow()

    init {
        loadConversations()
    }

    private fun loadConversations() {
        viewModelScope.launch {
            val savedConversations = settingsRepository.conversationsFlow.first()
            _uiState.update { it.copy(conversations = savedConversations) }
        }
    }

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
                error = null,
                thinking = ""
            )
        }

        viewModelScope.launch {
            try {
                val apiKey = settingsRepository.apiKeyFlow.first()
                val endpoint = settingsRepository.apiEndpointFlow.first()
                val model = settingsRepository.selectedModelFlow.first()
                val systemPrompt = settingsRepository.systemPromptFlow.first()

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

                val chatMessages = mutableListOf<ChatMessage>()
                
                // Add system prompt if present
                if (systemPrompt.isNotBlank()) {
                    chatMessages.add(ChatMessage(role = "system", content = systemPrompt))
                }
                
                // Add conversation history
                _uiState.value.messages.forEach { msg ->
                    chatMessages.add(
                        ChatMessage(
                            role = if (msg.isUser) "user" else "assistant",
                            content = msg.text
                        )
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
                                isLoading = false,
                                thinking = ""
                            )
                        }
                        saveConversation()
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
                                error = error.message,
                                thinking = ""
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
                        error = e.message,
                        thinking = ""
                    )
                }
            }
        }
    }

    fun clearChat() {
        _uiState.update { ChatUiState(conversations = _uiState.value.conversations) }
    }

    fun selectConversation(conversationId: String) {
        viewModelScope.launch {
            val conversation = _uiState.value.conversations.find { it.id == conversationId }
            conversation?.let {
                _uiState.update { state ->
                    state.copy(
                        messages = it.messages,
                        currentConversationId = conversationId
                    )
                }
            }
        }
    }

    fun createNewConversation() {
        _uiState.update { state ->
            state.copy(
                messages = emptyList(),
                currentConversationId = null
            )
        }
    }

    private fun saveConversation() {
        viewModelScope.launch {
            val currentMessages = _uiState.value.messages
            if (currentMessages.isEmpty()) return@launch

            val title = currentMessages.firstOrNull()?.text?.take(50) ?: "New Chat"
            val conversationId = _uiState.value.currentConversationId ?: java.util.UUID.randomUUID().toString()
            
            val conversation = Conversation(
                id = conversationId,
                title = title,
                messages = currentMessages,
                updatedAt = System.currentTimeMillis()
            )

            val updatedConversations = _uiState.value.conversations
                .filter { it.id != conversationId } + conversation

            settingsRepository.saveConversations(updatedConversations)
            
            _uiState.update { state ->
                state.copy(
                    conversations = updatedConversations,
                    currentConversationId = conversationId
                )
            }
        }
    }

    fun deleteConversation(conversationId: String) {
        viewModelScope.launch {
            val updatedConversations = _uiState.value.conversations.filter { it.id != conversationId }
            settingsRepository.saveConversations(updatedConversations)
            
            _uiState.update { state ->
                state.copy(conversations = updatedConversations)
            }
        }
    }
}

class ChatViewModelFactory(
    private val context: Context
) : androidx.lifecycle.ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T {
        return ChatViewModel(
            chatRepository = ChatRepository(),
            settingsRepository = SettingsRepository(context)
        ) as T
    }
}
