/**
 * AG-Claw Dashboard - Org Chart Page
 */

async function loadOrgChartData() {
  const container = document.getElementById('orgChartContainer');
  if (!container) return;

  container.innerHTML = `
    <div class="flex justify-center items-center" style="height: 200px">
      ${Components.spinner('lg')}
    </div>
  `;

  try {
    const response = await API.get('/api/org/chart');
    renderOrgChart(response);
  } catch (err) {
    renderOrgChart(getMockOrgData());
  }
}

function renderOrgChart(data) {
  const container = document.getElementById('orgChartContainer');
  if (!container) return;

  container.innerHTML = `
    <div class="card">
      <div class="card-header">
        <h3 class="card-title">Organization Structure</h3>
        <div class="flex gap-2">
          <button class="btn btn-secondary btn-sm" onclick="showHireModal()">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 14px; height: 14px">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            Hire Agent
          </button>
        </div>
      </div>
      
      <!-- ASCII Tree -->
      <div class="org-tree-container" style="margin-top: var(--space-4)">
        <pre class="org-ascii-tree">${renderAsciiTree(data.tree)}</pre>
      </div>
    </div>

    <!-- Stats Row -->
    <div class="stats-grid stagger-children" style="margin-top: var(--space-6)">
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="8" r="4"/><path d="M20 21a8 8 0 1 0-16 0"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${data.stats.totalAgents}</div>
        <div class="stat-card-label">Total Agents</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon success">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${data.stats.activeAgents}</div>
        <div class="stat-card-label">Active</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon warning">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${data.stats.pausedAgents}</div>
        <div class="stat-card-label">Paused</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon info">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="12" y1="1" x2="12" y2="23"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">$${((data.stats.totalBudget - data.stats.totalSpent) / 1000000).toFixed(2)}M</div>
        <div class="stat-card-label">Budget Remaining</div>
      </div>
    </div>

    <!-- Agent Cards -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Team Members</h3>
      <div class="agents-grid" style="margin-top: var(--space-4)">
        ${data.nodes.map(node => renderAgentCard(node)).join('')}
      </div>
    </div>

    <!-- Task Delegation -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Task Delegation</h3>
      <form id="taskDelegationForm" style="margin-top: var(--space-4)">
        <div class="form-grid">
          <div class="form-group">
            <label class="form-label">Task Description</label>
            <input type="text" class="input" name="taskDescription" placeholder="What needs to be done?">
          </div>
          <div class="form-group">
            <label class="form-label">Assign To</label>
            <select class="input select" name="agentId">
              <option value="">Select agent...</option>
              ${data.nodes.filter(n => n.status === 'active').map(n => 
                `<option value="${n.id}">${n.name} (${n.role})</option>`
              ).join('')}
            </select>
          </div>
          <div class="form-group">
            <label class="form-label">Priority (1-5)</label>
            <input type="number" class="input" name="priority" value="3" min="1" max="5">
          </div>
        </div>
        <button type="submit" class="btn btn-primary" style="margin-top: var(--space-4)">Assign Task</button>
      </form>
    </div>
  `;

  // Attach event handlers
  const form = document.getElementById('taskDelegationForm');
  if (form) {
    form.addEventListener('submit', handleTaskDelegation);
  }
}

function renderAsciiTree(tree) {
  if (!tree) return 'No organization data available.';
  
  const lines = [];
  
  function renderNode(node, prefix = '', isLast = true) {
    const connector = isLast ? '└── ' : '├── ';
    const statusIcon = node.status === 'active' ? '🟢' : node.status === 'paused' ? '🟡' : '⚫';
    const agentIcon = getAgentTypeIcon(node.agentType);
    
    lines.push(`${prefix}${connector}${statusIcon} ${agentIcon} ${node.name} <span class="text-muted">(${node.role})</span>`);
    
    const childPrefix = prefix + (isLast ? '    ' : '│   ');
    if (node.children && node.children.length > 0) {
      node.children.forEach((child, index) => {
        renderNode(child, childPrefix, index === node.children.length - 1);
      });
    }
  }
  
  lines.push(`⚡ AG-Claw Organization`);
  lines.push('');
  
  if (tree.children && tree.children.length > 0) {
    tree.children.forEach((child, index) => {
      renderNode(child, '', index === tree.children.length - 1);
    });
  } else {
    lines.push('└── ⚠️ No team members yet. Click "Hire Agent" to get started.');
  }
  
  return lines.join('\n');
}

function getAgentTypeIcon(agentType) {
  const icons = {
    'coder': '💻',
    'researcher': '🔬',
    'foreman': '👷',
    'analyst': '📊',
    'CTO': '🏛️',
    'default': '🤖'
  };
  return icons[agentType] || icons.default;
}

function renderAgentCard(node) {
  const statusClass = node.status === 'active' ? 'success' : node.status === 'paused' ? 'warning' : 'muted';
  const tasksCount = node.tasks ? node.tasks.filter(t => t.status !== 'completed').length : 0;
  
  return `
    <div class="agent-card" data-agent-id="${node.id}">
      <div class="agent-header">
        <div class="agent-avatar">${getAgentTypeIcon(node.agentType)}</div>
        <div class="agent-info">
          <div class="agent-name">${node.name}</div>
          <div class="agent-role">${node.role}</div>
          <div class="agent-status">
            ${Components.statusDot(statusClass)}
            ${node.status.charAt(0).toUpperCase() + node.status.slice(1)}
          </div>
        </div>
      </div>
      <div class="agent-stats">
        <div class="agent-stat">
          <span class="agent-stat-value">${tasksCount}</span>
          <span class="agent-stat-label">Pending Tasks</span>
        </div>
        <div class="agent-stat">
          <span class="agent-stat-value">${node.agentType}</span>
          <span class="agent-stat-label">Type</span>
        </div>
        <div class="agent-stat">
          <span class="agent-stat-value">${(node.id || '').slice(0, 8)}</span>
          <span class="agent-stat-label">ID</span>
        </div>
      </div>
      <div class="agent-actions" style="margin-top: var(--space-3)">
        ${node.status === 'active' ? `
          <button class="btn btn-warning btn-sm flex-1" onclick="pauseAgent('${node.id}')">
            Pause
          </button>
        ` : node.status === 'paused' ? `
          <button class="btn btn-success btn-sm flex-1" onclick="resumeAgent('${node.id}')">
            Resume
          </button>
        ` : ''}
        <button class="btn btn-danger btn-sm flex-1" onclick="showFireConfirmation('${node.id}', '${node.name}')">
          Fire
        </button>
      </div>
    </div>
  `;
}

async function handleTaskDelegation(e) {
  e.preventDefault();
  const form = e.target;
  const formData = new FormData(form);
  
  const task = {
    description: formData.get('taskDescription'),
    agentId: formData.get('agentId'),
    priority: parseInt(formData.get('priority')) || 3,
  };
  
  if (!task.description || !task.agentId) {
    Components.toast('Please fill in all fields', 'error');
    return;
  }
  
  try {
    await API.post('/api/org/assign', task);
    Components.toast('Task assigned successfully', 'success');
    form.reset();
    loadOrgChartData();
  } catch (err) {
    Components.toast('Failed to assign task', 'error');
  }
}

function showHireModal() {
  Components.modal({
    title: 'Hire New Agent',
    content: `
      <form id="hireAgentForm">
        <div class="form-group">
          <label class="form-label">Name</label>
          <input type="text" class="input" name="name" placeholder="Agent name" required>
        </div>
        <div class="form-group">
          <label class="form-label">Role</label>
          <select class="input select" name="role" required>
            <option value="CTO">CTO - Chief Technology Officer</option>
            <option value="Engineer">Engineer</option>
            <option value="Researcher">Researcher</option>
            <option value="Analyst">Analyst</option>
            <option value="Foreman">Foreman</option>
          </select>
        </div>
        <div class="form-group">
          <label class="form-label">Description (optional)</label>
          <input type="text" class="input" name="description" placeholder="Brief description">
        </div>
      </form>
    `,
    actions: [
      { label: 'Cancel', class: 'btn-secondary', action: 'close' },
      { label: 'Hire', class: 'btn-primary', action: 'hireAgent' },
    ],
  });
  
  // Store form reference
  window.pendingHireForm = document.getElementById('hireAgentForm');
}

async function hireAgent() {
  const form = window.pendingHireForm;
  if (!form) return;
  
  const formData = new FormData(form);
  
  try {
    await API.post('/api/org/hire', {
      name: formData.get('name'),
      role: formData.get('role'),
      description: formData.get('description'),
    });
    Components.toast('Agent hired successfully', 'success');
    Components.closeModal();
    loadOrgChartData();
  } catch (err) {
    Components.toast('Failed to hire agent', 'error');
  }
}

function showFireConfirmation(agentId, agentName) {
  Components.modal({
    title: 'Terminate Agent',
    content: `
      <div class="alert alert-danger">
        <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
          <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
        </svg>
        <span>This will permanently terminate <strong>${agentName}</strong>. This action cannot be undone.</span>
      </div>
      <p style="margin-top: var(--space-4)">Are you sure you want to proceed?</p>
    `,
    actions: [
      { label: 'Cancel', class: 'btn-secondary', action: 'close' },
      { label: 'Terminate', class: 'btn-danger', action: `fireAgent('${agentId}')` },
    ],
  });
}

async function fireAgent(agentId) {
  try {
    await API.post('/api/org/fire', { id: agentId });
    Components.toast('Agent terminated', 'success');
    Components.closeModal();
    loadOrgChartData();
  } catch (err) {
    Components.toast('Failed to terminate agent', 'error');
  }
}

async function pauseAgent(agentId) {
  try {
    await API.post('/api/org/pause', { id: agentId });
    Components.toast('Agent paused', 'success');
    loadOrgChartData();
  } catch (err) {
    Components.toast('Failed to pause agent', 'error');
  }
}

async function resumeAgent(agentId) {
  try {
    await API.post('/api/org/resume', { id: agentId });
    Components.toast('Agent resumed', 'success');
    loadOrgChartData();
  } catch (err) {
    Components.toast('Failed to resume agent', 'error');
  }
}

function getMockOrgData() {
  return {
    tree: {
      name: 'CEO',
      status: 'active',
      agentType: 'CTO',
      children: [
        {
          name: 'Alice',
          role: 'Engineer',
          status: 'active',
          agentType: 'coder',
          children: [
            { name: 'Bob', role: 'Engineer', status: 'active', agentType: 'coder', children: [] },
            { name: 'Carol', role: 'Engineer', status: 'paused', agentType: 'coder', children: [] },
          ],
        },
        {
          name: 'Dave',
          role: 'Researcher',
          status: 'active',
          agentType: 'researcher',
          children: [],
        },
        {
          name: 'Eve',
          role: 'Analyst',
          status: 'active',
          agentType: 'analyst',
          children: [],
        },
      ],
    },
    stats: {
      totalAgents: 5,
      activeAgents: 4,
      pausedAgents: 1,
      totalBudget: 10000000,
      totalSpent: 3470000,
    },
    nodes: [
      { id: 'agent-001', name: 'Alice', role: 'Engineer', status: 'active', agentType: 'coder', tasks: [{ status: 'in_progress' }, { status: 'completed' }] },
      { id: 'agent-002', name: 'Bob', role: 'Engineer', status: 'active', agentType: 'coder', tasks: [] },
      { id: 'agent-003', name: 'Carol', role: 'Engineer', status: 'paused', agentType: 'coder', tasks: [{ status: 'completed' }] },
      { id: 'agent-004', name: 'Dave', role: 'Researcher', status: 'active', agentType: 'researcher', tasks: [{ status: 'in_progress' }] },
      { id: 'agent-005', name: 'Eve', role: 'Analyst', status: 'active', agentType: 'analyst', tasks: [] },
    ],
  };
}

// Expose functions to global scope
window.showHireModal = showHireModal;
window.hireAgent = hireAgent;
window.fireAgent = fireAgent;
window.showFireConfirmation = showFireConfirmation;
window.pauseAgent = pauseAgent;
window.resumeAgent = resumeAgent;

// Org chart page initialization
document.addEventListener('DOMContentLoaded', () => {
  if (document.getElementById('page-org-chart')) {
    loadOrgChartData();
  }
});
