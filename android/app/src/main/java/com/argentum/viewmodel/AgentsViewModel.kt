package com.argentum.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.argentum.data.model.Result
import com.argentum.data.repository.AgentsRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

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
    val agents: List<Agent> = emptyList(),
    val selectedAgent: Agent? = null,
    val isLoading: Boolean = false,
    val error: String? = null
)

class AgentsViewModel(
    private val repository: AgentsRepository = AgentsRepository()
) : ViewModel() {

    private val _uiState = MutableStateFlow(AgentsUiState())
    val uiState: StateFlow<AgentsUiState> = _uiState.asStateFlow()

    init {
        loadAgents()
    }

    fun loadAgents() {
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true, error = null) }

            when (val result = repository.fetchAgents()) {
                is Result.Success -> {
                    _uiState.update { it.copy(agents = result.data, isLoading = false) }
                }
                is Result.Error -> {
                    // Fallback to placeholder data on error
                    _uiState.update {
                        it.copy(
                            agents = repository.getPlaceholderAgents(),
                            isLoading = false,
                            error = result.message
                        )
                    }
                }
                is Result.Loading -> {
                    _uiState.update { it.copy(isLoading = true) }
                }
            }
        }
    }

    fun selectAgent(agent: Agent) {
        _uiState.update { it.copy(selectedAgent = agent) }
    }

    fun clearError() {
        _uiState.update { it.copy(error = null) }
    }
}
