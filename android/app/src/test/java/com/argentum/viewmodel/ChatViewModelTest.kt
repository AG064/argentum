package com.argentum.viewmodel

import com.argentum.data.api.ChatMessage
import com.argentum.data.repository.ChatRepository
import com.argentum.data.repository.SettingsRepository
import io.mockk.coEvery
import io.mockk.coJustRun
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ChatViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before
    fun setUp() {
        Dispatchers.setMain(dispatcher)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `initial state has empty messages`() = runTest(dispatcher) {
        val viewModel = ChatViewModel(chatRepository(), settingsRepository())
        advanceUntilIdle()

        assertTrue(viewModel.uiState.value.messages.isEmpty())
        assertEquals("", viewModel.uiState.value.inputText)
        assertFalse(viewModel.uiState.value.isLoading)
    }

    @Test
    fun `onInputChange updates input text`() = runTest(dispatcher) {
        val viewModel = ChatViewModel(chatRepository(), settingsRepository())

        viewModel.onInputChange("Hello, Argentum!")

        assertEquals("Hello, Argentum!", viewModel.uiState.value.inputText)
    }

    @Test
    fun `sendMessage with empty text does nothing`() = runTest(dispatcher) {
        val viewModel = ChatViewModel(chatRepository(), settingsRepository())

        viewModel.sendMessage()
        advanceUntilIdle()

        assertTrue(viewModel.uiState.value.messages.isEmpty())
        assertEquals("", viewModel.uiState.value.inputText)
    }

    @Test
    fun `sendMessage reports missing API key without calling a provider`() = runTest(dispatcher) {
        val repository = settingsRepository(apiKey = "")
        val viewModel = ChatViewModel(chatRepository(), repository)

        viewModel.onInputChange("Hello!")
        viewModel.sendMessage()
        advanceUntilIdle()

        val messages = viewModel.uiState.value.messages
        assertEquals(2, messages.size)
        assertTrue(messages.first().isUser)
        assertTrue(messages.last().isError)
        assertEquals("API key not configured", viewModel.uiState.value.error)
    }

    @Test
    fun `sendMessage appends the provider response`() = runTest(dispatcher) {
        val sentMessages = mutableListOf<ChatMessage>()
        val repository = settingsRepository(apiKey = "test-key", systemPrompt = "Be concise")
        val chatRepository = chatRepository(response = Result.success("Hello from the provider")) {
            sentMessages += it
        }
        val viewModel = ChatViewModel(chatRepository, repository)

        viewModel.onInputChange("Hello!")
        viewModel.sendMessage()
        advanceUntilIdle()

        val messages = viewModel.uiState.value.messages
        assertEquals(2, messages.size)
        assertEquals("Hello!", messages.first().text)
        assertEquals("Hello from the provider", messages.last().text)
        assertFalse(messages.last().isUser)
        assertEquals(listOf("system", "user"), sentMessages.map { it.role })
        assertFalse(viewModel.uiState.value.isLoading)
    }

    private fun settingsRepository(
        apiKey: String = "test-key",
        systemPrompt: String = "",
    ): SettingsRepository = mockk {
        every { apiKeyFlow } returns MutableStateFlow(apiKey)
        every { apiEndpointFlow } returns MutableStateFlow("https://api.example.com/v1")
        every { selectedModelFlow } returns MutableStateFlow("test-model")
        every { systemPromptFlow } returns MutableStateFlow(systemPrompt)
        every { conversationsFlow } returns MutableStateFlow(emptyList())
        coJustRun { saveConversations(any()) }
    }

    private fun chatRepository(
        response: Result<String> = Result.success("unused"),
        onMessages: (List<ChatMessage>) -> Unit = {},
    ): ChatRepository {
        val messages = slot<List<ChatMessage>>()
        return mockk {
            coEvery { sendMessage(any(), any(), any(), capture(messages)) } answers {
                onMessages(messages.captured)
                response
            }
        }
    }
}
