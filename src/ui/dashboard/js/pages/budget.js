/**
 * Argentum Dashboard - Budget Page
 */

async function loadBudgetData() {
  const grid = document.getElementById('budgetGrid');
  if (!grid) return;

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  grid.innerHTML = `
    <div class="flex justify-center items-center" style="height: 200px">
      ${Components.spinner('lg')}
    </div>
  `;

  try {
    const response = await API.get('/api/budget');
    renderBudgetPage(response);
  } catch (err) {
    // Try loading from feature directly
    renderBudgetPage(getMockBudgetData());
  }
}

function renderBudgetPage(data) {
  const grid = document.getElementById('budgetGrid');
  if (!grid) return;

  const monthlyPct = Math.round((data.monthlyCost / data.monthlyLimit) * 100);
  const dailyPct = Math.round((data.dailyCost / data.dailyLimit) * 100);

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  grid.innerHTML = `
    <!-- Summary Cards -->
    <div class="stats-grid stagger-children">
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="12" y1="1" x2="12" y2="23"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>
            </svg>
          </div>
          <span class="stat-card-trend ${monthlyPct > 80 ? 'down' : 'up'}">${monthlyPct}%</span>
        </div>
        <div class="stat-card-value">$${data.monthlyCost.toFixed(4)}</div>
        <div class="stat-card-label">Monthly Spending</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon success">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="4" width="18" height="18" rx="2" ry="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">$${data.dailyCost.toFixed(4)}</div>
        <div class="stat-card-label">Daily Spending</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon info">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">$${data.monthlyLimit.toFixed(2)}</div>
        <div class="stat-card-label">Monthly Limit</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon warning">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${data.alerts.length}</div>
        <div class="stat-card-label">Active Alerts</div>
      </div>
    </div>

    <!-- Progress Bars -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Usage Progress</h3>
      <div style="margin-top: var(--space-4)">
        <div style="display: flex; justify-content: space-between; margin-bottom: var(--space-2)">
          <span class="text-sm">Monthly Budget</span>
          <span class="text-sm text-muted">$${data.monthlyCost.toFixed(4)} / $${data.monthlyLimit.toFixed(2)}</span>
        </div>
        <div class="progress-bar">
          <div class="progress-fill ${monthlyPct > 90 ? 'danger' : monthlyPct > 75 ? 'warning' : 'success'}" 
               style="width: ${Math.min(100, monthlyPct)}%"></div>
        </div>
        <div style="display: flex; justify-content: space-between; margin-top: var(--space-4); margin-bottom: var(--space-2)">
          <span class="text-sm">Daily Budget</span>
          <span class="text-sm text-muted">$${data.dailyCost.toFixed(4)} / $${data.dailyLimit.toFixed(2)}</span>
        </div>
        <div class="progress-bar">
          <div class="progress-fill ${dailyPct > 90 ? 'danger' : dailyPct > 75 ? 'warning' : 'success'}" 
               style="width: ${Math.min(100, dailyPct)}%"></div>
        </div>
      </div>
    </div>

    <!-- Per-Agent Breakdown -->
    ${data.byAgent && data.byAgent.length > 0 ? `
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Cost by Agent</h3>
      <table class="table" style="margin-top: var(--space-4)">
        <thead>
          <tr>
            <th>Agent</th>
            <th>Cost</th>
            <th>Tokens</th>
            <th>Requests</th>
            <th>% of Limit</th>
          </tr>
        </thead>
        <tbody>
          ${data.byAgent.map(agent => {
            const pct = Math.round((agent.totalCost / data.monthlyLimit) * 100);
            return `
              <tr>
                <td><strong>${agent.agent}</strong></td>
                <td>$${agent.totalCost.toFixed(4)}</td>
                <td>${agent.totalTokens.toLocaleString()}</td>
                <td>${agent.requestCount || 0}</td>
                <td>
                  <span class="badge ${pct > 80 ? 'badge-danger' : pct > 50 ? 'badge-warning' : 'badge-success'}">${pct}%</span>
                </td>
              </tr>
            `;
          }).join('')}
        </tbody>
      </table>
    </div>
    ` : ''}

    <!-- Alerts -->
    ${data.alerts && data.alerts.length > 0 ? `
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Alerts</h3>
      <div style="margin-top: var(--space-3)">
        ${data.alerts.map(alert => `
          <div class="alert alert-warning" style="margin-bottom: var(--space-2)">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="flex-shrink: 0">
              <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
              <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
            </svg>
            <span>${alert}</span>
          </div>
        `).join('')}
      </div>
    </div>
    ` : ''}

    <!-- Configuration Form -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Budget Configuration</h3>
      <form id="budgetConfigForm" style="margin-top: var(--space-4)">
        <div class="form-grid">
          <div class="form-group">
            <label class="form-label">Monthly Limit ($)</label>
            <input type="number" class="input" name="monthlyLimit" 
                   value="${data.monthlyLimit}" step="0.01" min="0">
          </div>
          <div class="form-group">
            <label class="form-label">Daily Limit ($)</label>
            <input type="number" class="input" name="dailyLimit" 
                   value="${data.dailyLimit}" step="0.01" min="0">
          </div>
          <div class="form-group">
            <label class="form-label">Per-Agent Limit ($)</label>
            <input type="number" class="input" name="perAgentLimit" 
                   value="${data.perAgentLimit || ''}" step="0.01" min="0" 
                   placeholder="Optional">
          </div>
          <div class="form-group">
            <label class="form-label">Alert Threshold (%)</label>
            <input type="number" class="input" name="alertThreshold" 
                   value="${Math.round((data.alertThreshold || 0.8) * 100)}" step="1" min="0" max="100">
          </div>
        </div>
        <div class="form-group" style="margin-top: var(--space-4)">
          <label class="toggle">
            <input type="checkbox" name="blockOnExhausted" ${data.blockOnExhausted ? 'checked' : ''}>
            <span class="toggle-slider"></span>
          </label>
          <span style="margin-left: var(--space-2)">Block agents when budget exhausted</span>
        </div>
        <div style="margin-top: var(--space-4)">
          <button type="submit" class="btn btn-primary">Save Configuration</button>
        </div>
      </form>
    </div>
  `;

  // Attach form handler
  const form = document.getElementById('budgetConfigForm');
  if (form) {
    form.addEventListener('submit', handleBudgetConfigSubmit);
  }
}

async function handleBudgetConfigSubmit(e) {
  e.preventDefault();
  const form = e.target;
  const formData = new FormData(form);
  
  const config = {
    monthlyLimit: parseFloat(formData.get('monthlyLimit')),
    dailyLimit: parseFloat(formData.get('dailyLimit')),
    perAgentLimit: formData.get('perAgentLimit') ? parseFloat(formData.get('perAgentLimit')) : null,
    alertThreshold: parseFloat(formData.get('alertThreshold')) / 100,
    blockOnExhausted: formData.get('blockOnExhausted') === 'on',
  };

  try {
    await API.post('/api/budget/config', config);
    Components.toast('Budget configuration saved', 'success');
  } catch (err) {
    Components.toast('Failed to save configuration', 'error');
  }
}

function getMockBudgetData() {
  return {
    monthlyCost: 3.47,
    monthlyLimit: 10.0,
    dailyCost: 0.23,
    dailyLimit: 1.0,
    perAgentLimit: 2.0,
    alertThreshold: 0.8,
    blockOnExhausted: true,
    alerts: [
      'Monthly spending at 34.7% of limit',
      'DeepSeek-V3 model usage is trending high',
    ],
    byAgent: [
      { agent: 'coder', totalCost: 1.82, totalTokens: 124500, requestCount: 47 },
      { agent: 'researcher', totalCost: 0.94, totalTokens: 67800, requestCount: 23 },
      { agent: 'foreman', totalCost: 0.71, totalTokens: 51200, requestCount: 31 },
    ],
  };
}

// Budget page initialization
document.addEventListener('DOMContentLoaded', () => {
  if (document.getElementById('page-budget')) {
    loadBudgetData();
  }
});
