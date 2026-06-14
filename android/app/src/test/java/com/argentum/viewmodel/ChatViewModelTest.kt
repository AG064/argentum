package com.argentum.viewmodel

import org.junit.Assert.*
import org.junit.Test

class ChatViewModelTest {

    @Test
    fun `initial state has empty messages`() {
        val viewModel = ChatViewModel()
        val state = viewModel.uiState.value
        
        assertTrue(state.messages.isEmpty())
        assertEquals("", state.inputText)
        assertFalse(state.isLoading)
    }

    @Test
    fun `onInputChange updates input text`() {
        val viewModel = ChatViewModel()
        
        viewModel.onInputChange("Hello, Argentum!")
        assertEquals("Hello, Argentum!", viewModel.uiState.value.inputText)
    }

    @Test
    fun `sendMessage with empty text does nothing`() {
        val viewModel = ChatViewModel()
        
        viewModel.sendMessage()
        
        assertTrue(viewModel.uiState.value.messages.isEmpty())
        assertEquals("", viewModel.uiState.value.inputText)
    }

    @Test
    fun `sendMessage adds user message and clears input`() {
        val viewModel = ChatViewModel()
        
        viewModel.onInputChange("Hello, Argentum!")
        viewModel.sendMessage()
        
        val messages = viewModel.uiState.value.messages
        assertEquals(1, messages.size)
        assertEquals("Hello, Argentum!", messages[0].text)
        assertTrue(messages[0].isUser)
        assertEquals("", viewModel.uiState.value.inputText)
    }

    @Test
    fun `sendMessage adds AI response after user message`() {
        val viewModel = ChatViewModel()
        
        viewModel.onInputChange("Hello!")
        viewModel.sendMessage()
        
        val messages = viewModel.uiState.value.messages
        assertEquals(2, messages.size)
        assertTrue(messages[0].isUser)
        assertFalse(messages[1].isUser)
    }

    @Test
    fun `multiple messages are appended correctly`() {
        val viewModel = ChatViewModel()
        
        viewModel.onInputChange("First message")
        viewModel.sendMessage()
        
        viewModel.onInputChange("Second message")
        viewModel.sendMessage()
        
        val messages = viewModel.uiState.value.messages
        assertEquals(4, messages.size) // 2 per sendMessage (user + AI)
        assertEquals("First message", messages[0].text)
        assertEquals("Second message", messages[2].text)
    }
}
