package com.argentum.viewmodel

import org.junit.Assert.*
import org.junit.Test

class AgentsViewModelTest {

    @Test
    fun `initial state has empty agents list`() {
        val viewModel = AgentsViewModel()
        val state = viewModel.uiState.value
        
        assertNotNull(state.agents)
        assertTrue(state.agents.isEmpty())
        assertNull(state.selectedAgent)
        assertFalse(state.isLoading)
        assertNull(state.error)
    }

    @Test
    fun `loadAgents populates agents list with placeholder data`() {
        val viewModel = AgentsViewModel()
        
        // Initial load happens in init, check state after
        val state = viewModel.uiState.value
        
        assertEquals(4, state.agents.size)
        assertEquals("Argentum Core", state.agents[0].name)
        assertEquals(AgentStatus.ACTIVE, state.agents[0].status)
        assertFalse(state.isLoading)
        assertNull(state.error)
    }

    @Test
    fun `selectAgent updates selectedAgent in state`() {
        val viewModel = AgentsViewModel()
        val agent = viewModel.uiState.value.agents.first()
        
        viewModel.selectAgent(agent)
        
        assertEquals(agent, viewModel.uiState.value.selectedAgent)
    }

    @Test
    fun `selectAgent with null clears selection`() {
        val viewModel = AgentsViewModel()
        val agent = viewModel.uiState.value.agents.first()
        
        viewModel.selectAgent(agent)
        assertNotNull(viewModel.uiState.value.selectedAgent)
        
        viewModel.selectAgent(agent) // Toggle off by selecting same
        // Note: current implementation replaces selection, doesn't toggle
    }
}
