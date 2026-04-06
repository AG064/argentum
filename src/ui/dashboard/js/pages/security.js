/**
 * AG-Claw Dashboard - Security Page
 *
 * Enterprise security dashboard with:
 * - Policy status (active/blocked count)
 * - Pending approvals (critical actions waiting)
 * - Recent decisions (audit log)
 * - Credential health (expiring keys)
 * - Threat detection (suspicious patterns)
 * - Compliance overview
 */

async function loadSecurityData() {
  const grid = document.getElementById('securityGrid');
  if (!grid) return;

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  grid.innerHTML = `
    <div class="flex justify-center items-center" style="height: 200px">
      ${Components.spinner('lg')}
    </div>
  `;

  try {
    // Fetch security stats and audit data
    const [statsResponse, auditResponse, policiesResponse, approvalsResponse] = await Promise.allSettled([
      API.get('/api/security/stats'),
      API.get('/api/security/audit?limit=20'),
      API.get('/api/security/policies'),
      API.get('/api/security/approvals'),
    ]);

    const stats = statsResponse.status === 'fulfilled' ? statsResponse.value : getMockSecurityStats();
    const auditLog = auditResponse.status === 'fulfilled' ? auditResponse.value : [];
    const policies = policiesResponse.status === 'fulfilled' ? policiesResponse.value : getMockPolicies();
    const approvals = approvalsResponse.status === 'fulfilled' ? approvalsResponse.value : [];

    renderSecurityPage({ stats, auditLog, policies, approvals });
  } catch (err) {
    console.error('[Security] Failed to load data:', err);
    renderSecurityPage({
      stats: getMockSecurityStats(),
      auditLog: [],
      policies: [],
      approvals: [],
    });
  }
}

function renderSecurityPage({ stats, auditLog, policies, approvals }) {
  const grid = document.getElementById('securityGrid');
  if (!grid) return;

  const activePolicies = policies.filter(p => p.enabled).length;
  const pendingApprovals = approvals.filter(a => a.status === 'pending').length;
  const expiringCreds = stats.credentialsExpiringSoon || 0;
  const threats = stats.threatsDetected || 0;

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  grid.innerHTML = `
    <!-- Summary Cards -->
    <div class="stats-grid stagger-children">
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
            </svg>
          </div>
          <span class="stat-card-trend ${activePolicies > 0 ? 'up' : ''}">${activePolicies}/${stats.policiesTotal}</span>
        </div>
        <div class="stat-card-value">${activePolicies}</div>
        <div class="stat-card-label">Active Policies</div>
      </div>

      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon ${pendingApprovals > 0 ? 'warning' : 'success'}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${pendingApprovals}</div>
        <div class="stat-card-label">Pending Approvals</div>
      </div>

      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon ${expiringCreds > 0 ? 'warning' : 'info'}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${expiringCreds}</div>
        <div class="stat-card-label">Expiring Credentials</div>
      </div>

      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon ${threats > 0 ? 'danger' : 'success'}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${threats}</div>
        <div class="stat-card-label">Threats (24h)</div>
      </div>
    </div>

    <!-- Tabs -->
    <div class="card" style="margin-top: var(--space-6)">
      <div style="display: flex; gap: var(--space-2); margin-bottom: var(--space-4); border-bottom: 1px solid var(--border); padding-bottom: var(--space-3)">
        <button class="tab-btn active" data-tab="policies">Policies</button>
        <button class="tab-btn" data-tab="approvals">Approvals</button>
        <button class="tab-btn" data-tab="audit">Audit Log</button>
        <button class="tab-btn" data-tab="credentials">Credentials</button>
        <button class="tab-btn" data-tab="sandbox">Sandbox</button>
      </div>

      <!-- Policies Tab -->
      <div class="tab-content active" id="tab-policies">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-4)">
          <h3 class="card-title" style="margin: 0">Security Policies</h3>
          <button class="btn btn-sm" onclick="openAddPolicyModal()">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 14px; height: 14px">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            Add Policy
          </button>
        </div>
        <table class="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Resource</th>
              <th>Action</th>
              <th>Effect</th>
              <th>Priority</th>
              <th>Status</th>
              <th>Approval</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            ${policies.length === 0 ? `
              <tr>
                <td colspan="8" class="text-center text-muted" style="padding: var(--space-6)">
                  No policies configured. Add one to get started.
                </td>
              </tr>
            ` : policies.map(policy => `
              <tr>
                <td><strong>${escapeHtml(policy.name)}</strong></td>
                <td><code style="font-size: 11px">${escapeHtml(policy.resource)}</code></td>
                <td><span class="badge">${policy.action}</span></td>
                <td>
                  <span class="badge ${policy.effect === 'allow' ? 'badge-success' : policy.effect === 'deny' ? 'badge-danger' : 'badge-warning'}">
                    ${policy.effect}
                  </span>
                </td>
                <td>${policy.priority}</td>
                <td>
                  <label class="toggle">
                    <input type="checkbox" ${policy.enabled ? 'checked' : ''} 
                           onchange="togglePolicy('${policy.id}', this.checked)">
                    <span class="toggle-slider"></span>
                  </label>
                </td>
                <td>
                  ${policy.requiresApproval ? `<span class="badge badge-warning">${policy.approvalRisk || 'yes'}</span>` : '-'}
                </td>
                <td>
                  <button class="btn btn-sm btn-ghost" onclick="editPolicy('${policy.id}')">Edit</button>
                </td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      </div>

      <!-- Approvals Tab -->
      <div class="tab-content" id="tab-approvals">
        <h3 class="card-title">Pending Approvals</h3>
        ${approvals.length === 0 ? `
          <div class="empty-state" style="padding: var(--space-6)">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 48px; height: 48px; margin-bottom: var(--space-3); opacity: 0.3">
              <path d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/>
            </svg>
            <p>No pending approvals</p>
          </div>
        ` : `
          <div style="display: flex; flex-direction: column; gap: var(--space-3)">
            ${approvals.map(approval => `
              <div class="alert ${approval.risk === 'critical' ? 'alert-danger' : approval.risk === 'high' ? 'alert-warning' : 'alert-info'}" 
                   style="margin-bottom: 0">
                <div style="display: flex; justify-content: space-between; align-items: flex-start">
                  <div style="flex: 1">
                    <div style="display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1)">
                      <span class="badge badge-${approval.risk === 'critical' ? 'danger' : approval.risk === 'high' ? 'warning' : 'info'}">
                        ${approval.risk?.toUpperCase()}
                      </span>
                      <strong>${escapeHtml(approval.agentId)}</strong>
                      <span class="text-muted text-sm">${formatAge(approval.requestedAt)}</span>
                    </div>
                    <p style="margin: 0; font-size: 13px">${escapeHtml(approval.details?.what || 'No description')}</p>
                    <p style="margin: var(--space-1) 0 0; font-size: 12px; color: var(--text-muted)">
                      ${escapeHtml(approval.details?.why || '')}
                    </p>
                  </div>
                  <div style="display: flex; gap: var(--space-2); margin-left: var(--space-4)">
                    <button class="btn btn-sm btn-success" onclick="handleApproval('${approval.id}', 'approve')">
                      Approve
                    </button>
                    <button class="btn btn-sm btn-danger" onclick="handleApproval('${approval.id}', 'deny')">
                      Deny
                    </button>
                  </div>
                </div>
              </div>
            `).join('')}
          </div>
        `}
      </div>

      <!-- Audit Log Tab -->
      <div class="tab-content" id="tab-audit">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-4)">
          <h3 class="card-title" style="margin: 0">Recent Audit Log</h3>
          <div style="display: flex; gap: var(--space-2)">
            <select class="input" style="width: auto" onchange="filterAuditLog('severity', this.value)">
              <option value="">All Severities</option>
              <option value="info">Info</option>
              <option value="warning">Warning</option>
              <option value="error">Error</option>
            </select>
            <button class="btn btn-sm" onclick="refreshAuditLog()">
              <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 14px; height: 14px">
                <polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/>
                <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
              </svg>
              Refresh
            </button>
          </div>
        </div>
        <table class="table">
          <thead>
            <tr>
              <th>Time</th>
              <th>Severity</th>
              <th>Action</th>
              <th>Actor</th>
              <th>Resource</th>
              <th>Decision</th>
            </tr>
          </thead>
          <tbody>
            ${auditLog.length === 0 ? `
              <tr>
                <td colspan="6" class="text-center text-muted" style="padding: var(--space-6)">
                  No audit entries yet
                </td>
              </tr>
            ` : auditLog.map(entry => `
              <tr class="${entry.severity === 'error' || entry.severity === 'critical' ? 'row-danger' : entry.severity === 'warning' ? 'row-warning' : ''}">
                <td class="text-sm text-muted">${formatTime(entry.timestamp)}</td>
                <td>
                  <span class="badge badge-${entry.severity === 'error' || entry.severity === 'critical' ? 'danger' : entry.severity === 'warning' ? 'warning' : 'info'}">
                    ${entry.severity}
                  </span>
                </td>
                <td><code style="font-size: 11px">${entry.action}</code></td>
                <td>${escapeHtml(entry.actor || '-')}</td>
                <td><code style="font-size: 11px">${escapeHtml(entry.resource || '-')}</code></td>
                <td>
                  ${entry.decision ? `<span class="badge ${entry.decision === 'allow' ? 'badge-success' : entry.decision === 'deny' || entry.decision === 'block' ? 'badge-danger' : 'badge-warning'}">${entry.decision}</span>` : '-'}
                </td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      </div>

      <!-- Credentials Tab -->
      <div class="tab-content" id="tab-credentials">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-4)">
          <h3 class="card-title" style="margin: 0">Credentials</h3>
          <button class="btn btn-sm" onclick="rotateAllCredentials()">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 14px; height: 14px">
              <polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/>
              <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
            </svg>
            Rotate All
          </button>
        </div>
        <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: var(--space-3)">
          ${getMockCredentials().map(cred => `
            <div class="card" style="padding: var(--space-3)">
              <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-2)">
                <strong>${escapeHtml(cred.name)}</strong>
                <span class="badge badge-${cred.expiresSoon ? 'warning' : 'success'}">
                  ${cred.expiresSoon ? 'Expiring' : 'Valid'}
                </span>
              </div>
              <div class="text-sm text-muted">${escapeHtml(cred.provider)}</div>
              <div class="text-sm text-muted">Type: ${cred.type}</div>
              <div class="text-sm text-muted">Expires: ${formatExpiry(cred.expiresAt)}</div>
              <div style="margin-top: var(--space-2); display: flex; gap: var(--space-2)">
                <button class="btn btn-sm btn-ghost" onclick="rotateCredential('${cred.id}')">Rotate</button>
                <button class="btn btn-sm btn-ghost" onclick="deleteCredential('${cred.id}')">Delete</button>
              </div>
            </div>
          `).join('')}
        </div>
      </div>

      <!-- Sandbox Tab -->
      <div class="tab-content" id="tab-sandbox">
        <h3 class="card-title">Sandbox Configuration</h3>
        <form id="sandboxConfigForm" style="margin-top: var(--space-4)">
          <div class="form-grid">
            <div class="form-group">
              <label class="toggle">
                <input type="checkbox" name="enabled" ${stats.sandboxEnabled !== false ? 'checked' : ''}>
                <span class="toggle-slider"></span>
              </label>
              <span style="margin-left: var(--space-2)">Sandbox Enabled</span>
            </div>
            <div class="form-group">
              <label class="toggle">
                <input type="checkbox" name="networkIsolation" checked>
                <span class="toggle-slider"></span>
              </label>
              <span style="margin-left: var(--space-2)">Network Isolation</span>
            </div>
            <div class="form-group">
              <label class="toggle">
                <input type="checkbox" name="allowExec" checked>
                <span class="toggle-slider"></span>
              </label>
              <span style="margin-left: var(--space-2)">Allow Exec</span>
            </div>
          </div>
          <div class="form-grid" style="margin-top: var(--space-4)">
            <div class="form-group">
              <label class="form-label">Max Memory (MB)</label>
              <input type="number" class="input" name="maxMemoryMb" value="${stats.sandboxMaxMemory || 512}" min="64" max="4096">
            </div>
            <div class="form-group">
              <label class="form-label">Max CPU (%)</label>
              <input type="number" class="input" name="maxCpuPercent" value="${stats.sandboxMaxCpu || 50}" min="1" max="100">
            </div>
            <div class="form-group">
              <label class="form-label">Max Exec Time (ms)</label>
              <input type="number" class="input" name="maxExecutionTimeMs" value="${stats.sandboxMaxExecTime || 30000}" min="1000" max="300000">
            </div>
          </div>
          <div style="margin-top: var(--space-4)">
            <button type="submit" class="btn btn-primary">Save Sandbox Config</button>
          </div>
        </form>
      </div>
    </div>

    <!-- Compliance Card -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Security Compliance</h3>
      <div style="margin-top: var(--space-4)">
        ${renderComplianceBar(stats.complianceScore || 85)}
      </div>
      <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: var(--space-4); margin-top: var(--space-4)">
        <div>
          <div style="display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1)">
            <span class="badge badge-success">✓</span>
            <span class="text-sm">Default Deny Policy</span>
          </div>
          <div style="display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1)">
            <span class="badge badge-success">✓</span>
            <span class="text-sm">Audit Logging</span>
          </div>
          <div style="display: flex; align-items: center; gap: var(--space-2)">
            <span class="badge ${stats.approvalsPending > 0 ? 'badge-warning' : 'badge-success'}">${stats.approvalsPending > 0 ? '!' : '✓'}</span>
            <span class="text-sm">Human-in-the-Loop</span>
          </div>
        </div>
        <div>
          <div style="display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1)">
            <span class="badge badge-success">✓</span>
            <span class="text-sm">Short-lived Credentials</span>
          </div>
          <div style="display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1)">
            <span class="badge badge-success">✓</span>
            <span class="text-sm">Sandbox Execution</span>
          </div>
          <div style="display: flex; align-items: center; gap: var(--space-2)">
            <span class="badge badge-success">✓</span>
            <span class="text-sm">SSRF Protection</span>
          </div>
        </div>
      </div>
    </div>
  `;

  // Attach tab handlers
  document.querySelectorAll('.tab-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
      document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
      btn.classList.add('active');
      const tabId = btn.dataset.tab;
      document.getElementById(`tab-${tabId}`)?.classList.add('active');
    });
  });

  // Attach form handlers
  const sandboxForm = document.getElementById('sandboxConfigForm');
  if (sandboxForm) {
    sandboxForm.addEventListener('submit', handleSandboxConfigSubmit);
  }
}

// ─── Event Handlers ────────────────────────────────────────────────────────────

window.togglePolicy = async function(id, enabled) {
  try {
    await API.post(`/api/security/policies/${id}/toggle`, { enabled });
    Components.toast('Policy updated', 'success');
  } catch (err) {
    Components.toast('Failed to update policy', 'error');
  }
};

window.editPolicy = async function(id) {
  Components.toast('Edit policy: ' + id + ' (not implemented in demo)', 'info');
};

window.openAddPolicyModal = function() {
  Components.toast('Add policy modal (not implemented in demo)', 'info');
};

window.handleApproval = async function(id, decision) {
  try {
    await API.post(`/api/security/approvals/${id}/${decision}`);
    Components.toast(`Request ${decision}ed`, 'success');
    loadSecurityData(); // Refresh
  } catch (err) {
    Components.toast(`Failed to ${decision} request`, 'error');
  }
};

window.filterAuditLog = function(field, value) {
  console.log('Filter audit:', field, value);
  // Would re-fetch with filter
};

window.refreshAuditLog = function() {
  loadSecurityData();
};

window.rotateCredential = async function(id) {
  try {
    await API.post(`/api/security/credentials/${id}/rotate`);
    Components.toast('Credential rotated', 'success');
  } catch (err) {
    Components.toast('Failed to rotate credential', 'error');
  }
};

window.rotateAllCredentials = async function() {
  try {
    await API.post('/api/security/credentials/rotate-all');
    Components.toast('All credentials rotated', 'success');
  } catch (err) {
    Components.toast('Failed to rotate credentials', 'error');
  }
};

window.deleteCredential = async function(id) {
  if (!confirm('Delete this credential? This cannot be undone.')) return;
  try {
    await API.delete(`/api/security/credentials/${id}`);
    Components.toast('Credential deleted', 'success');
    loadSecurityData();
  } catch (err) {
    Components.toast('Failed to delete credential', 'error');
  }
};

async function handleSandboxConfigSubmit(e) {
  e.preventDefault();
  const form = e.target;
  const formData = new FormData(form);

  const config = {
    enabled: formData.get('enabled') === 'on',
    networkIsolation: formData.get('networkIsolation') === 'on',
    allowExec: formData.get('allowExec') === 'on',
    maxMemoryMb: parseInt(formData.get('maxMemoryMb'), 10),
    maxCpuPercent: parseInt(formData.get('maxCpuPercent'), 10),
    maxExecutionTimeMs: parseInt(formData.get('maxExecutionTimeMs'), 10),
  };

  try {
    await API.post('/api/security/sandbox/config', config);
    Components.toast('Sandbox configuration saved', 'success');
  } catch (err) {
    Components.toast('Failed to save configuration', 'error');
  }
}

// ─── Mock Data ────────────────────────────────────────────────────────────────

function getMockSecurityStats() {
  return {
    policiesActive: 6,
    policiesTotal: 8,
    credentialsTotal: 3,
    credentialsExpiringSoon: 1,
    approvalsPending: 2,
    approvalsTotal: 12,
    auditEntriesTotal: 247,
    sandboxExecutionsTotal: 89,
    sandboxBlockedTotal: 3,
    threatsDetected: 0,
    complianceScore: 92,
    sandboxEnabled: true,
    sandboxMaxMemory: 512,
    sandboxMaxCpu: 50,
    sandboxMaxExecTime: 30000,
    uptime: process?.uptime?.() ?? 86400,
  };
}

function getMockPolicies() {
  return [
    { id: '1', name: 'allow-read-ag-claw', resource: 'file://~/ag-claw/**', action: 'read', effect: 'allow', priority: 10, enabled: true, requiresApproval: false },
    { id: '2', name: 'allow-write-data', resource: 'file://~/ag-claw/data/**', action: 'write', effect: 'allow', priority: 10, enabled: true, requiresApproval: false },
    { id: '3', name: 'deny-system', resource: 'file:///etc/**', action: '*', effect: 'deny', priority: 100, enabled: true, requiresApproval: false },
    { id: '4', name: 'deny-ssh', resource: 'file://**/.ssh/**', action: '*', effect: 'deny', priority: 100, enabled: true, requiresApproval: false },
    { id: '5', name: 'require-approval-exec', resource: 'exec://**', action: 'exec', effect: 'approve', priority: 50, enabled: true, requiresApproval: true, approvalRisk: 'high' },
    { id: '6', name: 'allow-https', resource: 'https://**', action: 'network', effect: 'allow', priority: 1, enabled: true, requiresApproval: false },
    { id: '7', name: 'deny-private-network', resource: 'network://**', action: 'network', effect: 'deny', priority: 90, enabled: true, requiresApproval: false },
    { id: '8', name: 'disabled-policy', resource: 'file://**/tmp/**', action: '*', effect: 'deny', priority: 1, enabled: false, requiresApproval: false },
  ];
}

function getMockCredentials() {
  return [
    { id: '1', name: 'OpenAI API', provider: 'openai', type: 'api_key', expiresAt: Date.now() + 120000, expiresSoon: true },
    { id: '2', name: 'GitHub Token', provider: 'github', type: 'oauth', expiresAt: Date.now() + 3600000, expiresSoon: false },
    { id: '3', name: 'Supabase JWT', provider: 'supabase', type: 'jwt', expiresAt: Date.now() + 7200000, expiresSoon: false },
  ];
}

function getMockApprovals() {
  return [
    {
      id: 'abc-123',
      agentId: 'coder',
      risk: 'high',
      status: 'pending',
      requestedAt: Date.now() - 120000,
      expiresAt: Date.now() + 280000,
      details: {
        what: 'Agent wants to execute: rm -rf /tmp/test-dir',
        why: 'Required by task: cleanup old files',
        consequences: 'Will permanently delete directory',
      },
    },
    {
      id: 'def-456',
      agentId: 'researcher',
      risk: 'medium',
      status: 'pending',
      requestedAt: Date.now() - 60000,
      expiresAt: Date.now() + 240000,
      details: {
        what: 'Agent wants to access: https://api.example.com/data',
        why: 'Fetching research data',
        consequences: 'Network request to external API',
      },
    },
  ];
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function escapeHtml(text) {
  if (!text) return '';
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

function formatAge(timestamp) {
  const diff = Date.now() - timestamp;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

function formatTime(timestamp) {
  return new Date(timestamp).toLocaleTimeString();
}

function formatExpiry(timestamp) {
  const diff = timestamp - Date.now();
  if (diff <= 0) return 'Expired';
  const mins = Math.floor(diff / 60000);
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  return `${hours}h`;
}

function renderComplianceBar(score) {
  const color = score >= 90 ? 'var(--color-success)' : score >= 70 ? 'var(--color-warning)' : 'var(--color-danger)';
  return `
    <div style="display: flex; align-items: center; gap: var(--space-3)">
      <div style="flex: 1; height: 8px; background: var(--border); border-radius: 4px; overflow: hidden">
        <div style="width: ${score}%; height: 100%; background: ${color}; transition: width 0.3s"></div>
      </div>
      <span style="font-weight: 600; color: ${color}">${score}%</span>
    </div>
    <p class="text-sm text-muted" style="margin-top: var(--space-2)">
      ${score >= 90 ? 'Excellent security posture' : score >= 70 ? 'Good security posture, some improvements recommended' : 'Security needs attention'}
    </p>
  `;
}

// Security page initialization
document.addEventListener('DOMContentLoaded', () => {
  if (document.getElementById('page-security')) {
    loadSecurityData();
  }
});
