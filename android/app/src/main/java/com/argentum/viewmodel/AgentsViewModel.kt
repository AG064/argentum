package com.argentum.viewmodel

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

data class Agent(
    val id: String,
    val name: String,
    val description: String,
    val status: AgentStatus = AgentStatus.IDLE,
    val icon: String = "🤖"
)

enum class AgentStatus {
    IDLE, ACTIVE, BUSY, OFFLINE
}

data class AgentsUiState(
    val agents: List<Agent> = listOf(
        Agent("1", "Argentum Core", "Main AI agent for general tasks", AgentStatus.ACTIVE, "🤖"),
        Agent("2", "Code Assistant", "Specialized in code review and suggestions", AgentStatus.IDLE, "💻"),
        Agent("3", "Research Agent", "Web search and information gathering", AgentStatus.BUSY, "🔍"),
        Agent("4", "Memory Keeper", "Manages persistent memory and context", AgentStatus.IDLE, "🧠")
    ),
    val selectedAgent: Agent? = null
)

class AgentsViewModel : ViewModel() {
    private val _uiState = MutableStateFlow(AgentsUiState())
    val uiState: StateFlow<AgentsUiState> = _uiState.asStateFlow()

    fun selectAgent(agent: Agent) {
        _uiState.update { it.copy(selectedAgent = agent) }
    }
}
