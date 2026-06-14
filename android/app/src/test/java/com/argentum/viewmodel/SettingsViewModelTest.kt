package com.argentum.viewmodel

import org.junit.Assert.*
import org.junit.Test

class SettingsViewModelTest {

    @Test
    fun `initial state has default values`() {
        val viewModel = SettingsViewModel()
        val state = viewModel.uiState.value
        
        assertTrue(state.isDarkMode)
        assertEquals("MiniMax-M2.7", state.selectedModel)
        assertEquals(4, state.availableModels.size)
        assertEquals("https://api.minimax.io", state.apiEndpoint)
        assertTrue(state.notificationsEnabled)
    }

    @Test
    fun `toggleDarkMode toggles dark mode state`() {
        val viewModel = SettingsViewModel()
        
        assertTrue(viewModel.uiState.value.isDarkMode)
        
        viewModel.toggleDarkMode()
        assertFalse(viewModel.uiState.value.isDarkMode)
        
        viewModel.toggleDarkMode()
        assertTrue(viewModel.uiState.value.isDarkMode)
    }

    @Test
    fun `selectModel updates selected model`() {
        val viewModel = SettingsViewModel()
        
        viewModel.selectModel("GPT-4o")
        assertEquals("GPT-4o", viewModel.uiState.value.selectedModel)
    }

    @Test
    fun `updateApiEndpoint updates API endpoint`() {
        val viewModel = SettingsViewModel()
        
        viewModel.updateApiEndpoint("https://api.example.com")
        assertEquals("https://api.example.com", viewModel.uiState.value.apiEndpoint)
    }

    @Test
    fun `toggleNotifications toggles notifications state`() {
        val viewModel = SettingsViewModel()
        
        assertTrue(viewModel.uiState.value.notificationsEnabled)
        
        viewModel.toggleNotifications()
        assertFalse(viewModel.uiState.value.notificationsEnabled)
        
        viewModel.toggleNotifications()
        assertTrue(viewModel.uiState.value.notificationsEnabled)
    }
}
