package com.argentum.viewmodel

import com.argentum.data.repository.SettingsRepository
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.*
import org.junit.Test

class FakeSettingsRepository : SettingsRepository {
    private val _darkMode = MutableStateFlow(true)
    private val _selectedModel = MutableStateFlow("MiniMax-M2.7")
    private val _apiEndpoint = MutableStateFlow("https://api.minimax.io")
    private val _notifications = MutableStateFlow(true)

    override val darkModeFlow: Flow<Boolean> = _darkMode
    override val selectedModelFlow: Flow<String> = _selectedModel
    override val apiEndpointFlow: Flow<String> = _apiEndpoint
    override val notificationsFlow: Flow<Boolean> = _notifications

    override suspend fun setDarkMode(enabled: Boolean) { _darkMode.value = enabled }
    override suspend fun setSelectedModel(model: String) { _selectedModel.value = model }
    override suspend fun setApiEndpoint(endpoint: String) { _apiEndpoint.value = endpoint }
    override suspend fun setNotificationsEnabled(enabled: Boolean) { _notifications.value = enabled }
}

class SettingsViewModelTest {

    @Test
    fun `initial state has default values`() {
        val repository = FakeSettingsRepository()
        val viewModel = SettingsViewModel(repository)
        val state = viewModel.uiState.value
        
        assertTrue(state.isDarkMode)
        assertEquals("MiniMax-M2.7", state.selectedModel)
        assertEquals(4, state.availableModels.size)
        assertEquals("https://api.minimax.io", state.apiEndpoint)
        assertTrue(state.notificationsEnabled)
    }

    @Test
    fun `toggleDarkMode toggles dark mode state`() {
        val repository = FakeSettingsRepository()
        val viewModel = SettingsViewModel(repository)
        
        assertTrue(viewModel.uiState.value.isDarkMode)
        
        viewModel.toggleDarkMode()
        assertFalse(viewModel.uiState.value.isDarkMode)
        
        viewModel.toggleDarkMode()
        assertTrue(viewModel.uiState.value.isDarkMode)
    }

    @Test
    fun `selectModel updates selected model`() {
        val repository = FakeSettingsRepository()
        val viewModel = SettingsViewModel(repository)
        
        viewModel.selectModel("GPT-4o")
        assertEquals("GPT-4o", viewModel.uiState.value.selectedModel)
    }

    @Test
    fun `updateApiEndpoint updates API endpoint`() {
        val repository = FakeSettingsRepository()
        val viewModel = SettingsViewModel(repository)
        
        viewModel.updateApiEndpoint("https://api.example.com")
        assertEquals("https://api.example.com", viewModel.uiState.value.apiEndpoint)
    }

    @Test
    fun `toggleNotifications toggles notifications state`() {
        val repository = FakeSettingsRepository()
        val viewModel = SettingsViewModel(repository)
        
        assertTrue(viewModel.uiState.value.notificationsEnabled)
        
        viewModel.toggleNotifications()
        assertFalse(viewModel.uiState.value.notificationsEnabled)
        
        viewModel.toggleNotifications()
        assertTrue(viewModel.uiState.value.notificationsEnabled)
    }
}
