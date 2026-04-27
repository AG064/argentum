/**
 * Argentum Dashboard - Agents Page
 */

// Mock agent data
const mockAgents = [
  {
    id: 'coder',
    name: 'Coder',
    role: 'Software Development Agent',
    emoji: '👨‍💻',
    status: 'online',
    tasksCompleted: 1247,
    successRate: 99.2,
    avgRuntime: '2.3h',
    memory: '890 MB',
    uptime: '99.9%',
  },
  {
    id: 'researcher',
    name: 'Researcher',
    role: 'Research & Analysis Agent',
    emoji: '🔬',
    status: 'online',
    tasksCompleted: 843,
    successRate: 98.7,
    avgRuntime: '1.8h',
    memory: '456 MB',
    uptime: '99.5%',
  },
  {
    id: 'foreman',
    name: 'Foreman',
    role: 'Orchestration Agent',
    emoji: '👷',
    status: 'online',
    tasksCompleted: 432,
    successRate: 99.8,
    avgRuntime: '45m',
    memory: '234 MB',
    uptime: '99.9%',
  },
  {
    id: 'writer',
    name: 'Writer',
    role: 'Content Creation Agent',
    emoji: '✍️',
    status: 'idle',
    tasksCompleted: 156,
    successRate: 97.4,
    avgRuntime: '1.2h',
    memory: '178 MB',
    uptime: '98.2%',
  },
];

function loadAgents() {
  const grid = document.getElementById('agentsGrid');
  if (!grid) return;

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  grid.innerHTML = mockAgents
    .map(
      (agent) => `
    <div class="agent-card" onclick="openAgentDetail('${agent.id}')">
      <div class="agent-header">
        <div class="agent-avatar">${agent.emoji}</div>
        <div class="agent-info">
          <div class="agent-name">${agent.name}</div>
          <div class="agent-role">${agent.role}</div>
          <div class="agent-status">
            ${Components.statusDot(agent.status)}
            ${agent.status.charAt(0).toUpperCase() + agent.status.slice(1)}
          </div>
        </div>
      </div>
      
      <div class="agent-stats">
        <div class="agent-stat">
          <span class="agent-stat-value">${Components.formatNumber(agent.tasksCompleted)}</span>
          <span class="agent-stat-label">Tasks</span>
        </div>
        <div class="agent-stat">
          <span class="agent-stat-value">${agent.successRate}%</span>
          <span class="agent-stat-label">Success</span>
        </div>
        <div class="agent-stat">
          <span class="agent-stat-value">${agent.memory}</span>
          <span class="agent-stat-label">Memory</span>
        </div>
      </div>
      
      <div class="agent-actions">
        <button class="btn btn-success btn-sm" onclick="event.stopPropagation(); startAgent('${agent.id}')" ${agent.status === 'online' ? 'disabled' : ''}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polygon points="5 3 19 12 5 21 5 3"/>
          </svg>
          Start
        </button>
        <button class="btn btn-warning btn-sm" onclick="event.stopPropagation(); pauseAgent('${agent.id}')" ${agent.status !== 'online' ? 'disabled' : ''}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="6" y="4" width="4" height="16"/>
            <rect x="14" y="4" width="4" height="16"/>
          </svg>
          Pause
        </button>
        <button class="btn btn-secondary btn-sm" onclick="event.stopPropagation(); restartAgent('${agent.id}')">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="23 4 23 10 17 10"/>
            <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
          </svg>
        </button>
        <button class="btn btn-ghost btn-sm" onclick="event.stopPropagation();">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="1"/>
            <circle cx="19" cy="12" r="1"/>
            <circle cx="5" cy="12" r="1"/>
          </svg>
        </button>
      </div>
    </div>
  `,
    )
    .join('');
}

async function startAgent(agentId) {
  Components.toast(`Starting agent ${agentId}...`, 'info');
  // Simulate API call
  setTimeout(() => {
    Components.toast(`Agent ${agentId} started`, 'success');
    loadAgents();
  }, 1000);
}

async function pauseAgent(agentId) {
  Components.toast(`Pausing agent ${agentId}...`, 'info');
  setTimeout(() => {
    Components.toast(`Agent ${agentId} paused`, 'success');
    loadAgents();
  }, 1000);
}

async function restartAgent(agentId) {
  Components.toast(`Restarting agent ${agentId}...`, 'info');
  setTimeout(() => {
    Components.toast(`Agent ${agentId} restarted`, 'success');
    loadAgents();
  }, 1500);
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
  // Agents will be loaded when page is navigated to
});
