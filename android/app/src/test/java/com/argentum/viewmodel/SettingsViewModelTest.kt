package com.argentum.viewmodel

import com.argentum.data.repository.SettingsRepository
import io.mockk.coEvery
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
class SettingsViewModelTest {
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
    fun `initial state reflects persisted settings`() = runTest(dispatcher) {
        val viewModel = SettingsViewModel(settingsRepository())
        advanceUntilIdle()

        val state = viewModel.uiState.value
        assertTrue(state.isDarkMode)
        assertEquals("MiniMax-M2.7", state.selectedModel)
        assertEquals("https://api.minimax.io", state.apiEndpoint)
        assertTrue(state.notificationsEnabled)
    }

    @Test
    fun `toggleDarkMode persists the opposite state`() = runTest(dispatcher) {
        val darkMode = MutableStateFlow(true)
        val viewModel = SettingsViewModel(settingsRepository(darkMode = darkMode))
        advanceUntilIdle()

        viewModel.toggleDarkMode()
        advanceUntilIdle()

        assertFalse(darkMode.value)
        assertFalse(viewModel.uiState.value.isDarkMode)
    }

    @Test
    fun `selectModel persists the selected model`() = runTest(dispatcher) {
        val model = MutableStateFlow("MiniMax-M2.7")
        val viewModel = SettingsViewModel(settingsRepository(model = model))
        advanceUntilIdle()

        viewModel.selectModel("GPT-4o")
        advanceUntilIdle()

        assertEquals("GPT-4o", model.value)
        assertEquals("GPT-4o", viewModel.uiState.value.selectedModel)
    }

    @Test
    fun `updateApiEndpoint persists the endpoint`() = runTest(dispatcher) {
        val endpoint = MutableStateFlow("https://api.minimax.io")
        val viewModel = SettingsViewModel(settingsRepository(endpoint = endpoint))
        advanceUntilIdle()

        viewModel.updateApiEndpoint("https://api.example.com/v1")
        advanceUntilIdle()

        assertEquals("https://api.example.com/v1", endpoint.value)
        assertEquals("https://api.example.com/v1", viewModel.uiState.value.apiEndpoint)
    }

    @Test
    fun `toggleNotifications persists the opposite state`() = runTest(dispatcher) {
        val notifications = MutableStateFlow(true)
        val viewModel = SettingsViewModel(settingsRepository(notifications = notifications))
        advanceUntilIdle()

        viewModel.toggleNotifications()
        advanceUntilIdle()

        assertFalse(notifications.value)
        assertFalse(viewModel.uiState.value.notificationsEnabled)
    }

    private fun settingsRepository(
        darkMode: MutableStateFlow<Boolean> = MutableStateFlow(true),
        model: MutableStateFlow<String> = MutableStateFlow("MiniMax-M2.7"),
        endpoint: MutableStateFlow<String> = MutableStateFlow("https://api.minimax.io"),
        notifications: MutableStateFlow<Boolean> = MutableStateFlow(true),
    ): SettingsRepository {
        val darkModeValue = slot<Boolean>()
        val modelValue = slot<String>()
        val endpointValue = slot<String>()
        val notificationsValue = slot<Boolean>()
        return mockk {
            every { darkModeFlow } returns darkMode
            every { selectedProviderFlow } returns MutableStateFlow("minimax")
            every { selectedModelFlow } returns model
            every { apiEndpointFlow } returns endpoint
            every { apiKeyFlow } returns MutableStateFlow("")
            every { notificationsEnabledFlow } returns notifications
            every { systemPromptFlow } returns MutableStateFlow("")
            every { localServerUrlFlow } returns MutableStateFlow("http://127.0.0.1:8080/v1")
            coEvery { setDarkMode(capture(darkModeValue)) } answers { darkMode.value = darkModeValue.captured }
            coEvery { setSelectedModel(capture(modelValue)) } answers { model.value = modelValue.captured }
            coEvery { setApiEndpoint(capture(endpointValue)) } answers { endpoint.value = endpointValue.captured }
            coEvery { setNotificationsEnabled(capture(notificationsValue)) } answers {
                notifications.value = notificationsValue.captured
            }
        }
    }
}
