package com.argentum.viewmodel

import com.argentum.data.model.Result
import com.argentum.data.repository.AgentsRepository
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AgentsViewModelTest {
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
    fun `loadAgents publishes successful repository results`() = runTest(dispatcher) {
        val agents = listOf(Agent("core", "Argentum Core", "Primary agent", AgentStatus.ACTIVE))
        val viewModel = AgentsViewModel(repository(Result.Success(agents)))
        advanceUntilIdle()

        assertEquals(agents, viewModel.uiState.value.agents)
        assertFalse(viewModel.uiState.value.isLoading)
        assertNull(viewModel.uiState.value.error)
    }

    @Test
    fun `loadAgents exposes an error and uses the repository fallback`() = runTest(dispatcher) {
        val fallback = listOf(Agent("offline", "Offline agent", "Fallback", AgentStatus.OFFLINE))
        val viewModel = AgentsViewModel(repository(Result.Error("Network unavailable"), fallback))
        advanceUntilIdle()

        assertEquals(fallback, viewModel.uiState.value.agents)
        assertEquals("Network unavailable", viewModel.uiState.value.error)
        assertFalse(viewModel.uiState.value.isLoading)
    }

    @Test
    fun `selectAgent updates selected agent`() = runTest(dispatcher) {
        val agent = Agent("core", "Argentum Core", "Primary agent", AgentStatus.ACTIVE)
        val viewModel = AgentsViewModel(repository(Result.Success(listOf(agent))))
        advanceUntilIdle()

        viewModel.selectAgent(agent)

        assertEquals(agent, viewModel.uiState.value.selectedAgent)
    }

    private fun repository(
        result: Result<List<Agent>>,
        fallback: List<Agent> = emptyList(),
    ): AgentsRepository = mockk {
        coEvery { fetchAgents() } returns result
        every { getPlaceholderAgents() } returns fallback
    }
}
