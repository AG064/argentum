/**
 * AG-Claw Dashboard - Trajectory Export Page
 */

async function loadTrajectoryData() {
  const container = document.getElementById('trajectoryContainer');
  if (!container) return;

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  container.innerHTML = `
    <div class="flex justify-center items-center" style="height: 200px">
      ${Components.spinner('lg')}
    </div>
  `;

  try {
    const response = await API.get('/api/trajectory');
    renderTrajectoryPage(response);
  } catch (err) {
    renderTrajectoryPage(getMockTrajectoryData());
  }
}

function renderTrajectoryPage(data) {
  const container = document.getElementById('trajectoryContainer');
  if (!container) return;

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  container.innerHTML = `
    <!-- Stats Row -->
    <div class="stats-grid stagger-children">
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${data.stats.totalSessions.toLocaleString()}</div>
        <div class="stat-card-label">Total Sessions</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon success">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
              <polyline points="14 2 14 8 20 8"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${data.stats.totalMessages.toLocaleString()}</div>
        <div class="stat-card-label">Total Messages</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon info">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="12" y1="1" x2="12" y2="23"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">$${data.stats.totalCost.toFixed(4)}</div>
        <div class="stat-card-label">Total Cost</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-header">
          <div class="stat-card-icon warning">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>
            </svg>
          </div>
        </div>
        <div class="stat-card-value">${data.stats.totalTokens.toLocaleString()}</div>
        <div class="stat-card-label">Total Tokens</div>
      </div>
    </div>

    <!-- Export Form -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Export Trajectory</h3>
      <form id="exportTrajectoryForm" style="margin-top: var(--space-4)">
        <div class="form-grid">
          <div class="form-group">
            <label class="form-label">Session</label>
            <select class="input select" name="sessionId" id="sessionSelect">
              <option value="">Select a session...</option>
              ${data.sessions.map(s => `
                <option value="${s.id}">${s.title} (${s.messageCount} msgs)</option>
              `).join('')}
            </select>
          </div>
          <div class="form-group">
            <label class="form-label">Format</label>
            <select class="input select" name="format">
              <option value="jsonl">JSONL (JSON Lines)</option>
              <option value="json">JSON (formatted)</option>
            </select>
          </div>
          <div class="form-group">
            <label class="form-label">&nbsp;</label>
            <label class="toggle" style="margin-top: var(--space-2)">
              <input type="checkbox" name="gzip">
              <span class="toggle-slider"></span>
            </label>
            <span style="margin-left: var(--space-2)">Compress (gzip)</span>
          </div>
        </div>
        <div style="margin-top: var(--space-4)">
          <button type="submit" class="btn btn-primary" id="exportBtn">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 16px; height: 16px">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <polyline points="7 10 12 15 17 10"/>
              <line x1="12" y1="15" x2="12" y2="3"/>
            </svg>
            Export
          </button>
          <button type="button" class="btn btn-secondary" onclick="exportAllSessions()">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 16px; height: 16px">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <polyline points="7 10 12 15 17 10"/>
              <line x1="12" y1="15" x2="12" y2="3"/>
            </svg>
            Export All
          </button>
        </div>
      </form>
    </div>

    <!-- Session Selector (Alternative) -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Sessions</h3>
      <div class="table-container" style="margin-top: var(--space-4)">
        <table class="table">
          <thead>
            <tr>
              <th>Session</th>
              <th>Messages</th>
              <th>Tokens</th>
              <th>Cost</th>
              <th>Updated</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            ${data.sessions.map(s => `
              <tr>
                <td>
                  <strong>${s.title}</strong>
                  <div class="text-sm text-muted">${s.id.slice(0, 12)}...</div>
                </td>
                <td>${s.messageCount}</td>
                <td>${s.tokens?.toLocaleString() || '—'}</td>
                <td>$${s.cost?.toFixed(4) || '—'}</td>
                <td>${formatAge(s.updatedAt)}</td>
                <td>
                  <button class="btn btn-ghost btn-sm" onclick="selectSession('${s.id}')">Select</button>
                  <button class="btn btn-ghost btn-sm" onclick="exportSession('${s.id}')">Export</button>
                </td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      </div>
    </div>

    <!-- Export History -->
    ${data.exportHistory && data.exportHistory.length > 0 ? `
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Export History</h3>
      <div class="table-container" style="margin-top: var(--space-4)">
        <table class="table">
          <thead>
            <tr>
              <th>Time</th>
              <th>File</th>
              <th>Format</th>
              <th>Size</th>
              <th>Sessions</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            ${data.exportHistory.map(exp => `
              <tr>
                <td>${formatAge(exp.timestamp)}</td>
                <td><code class="code">${exp.filename}</code></td>
                <td><span class="badge badge-muted">${exp.format.toUpperCase()}</span></td>
                <td>${formatFileSize(exp.size)}</td>
                <td>${exp.sessionCount}</td>
                <td>
                  <button class="btn btn-ghost btn-sm" onclick="downloadExport('${exp.id}')">Download</button>
                </td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      </div>
    </div>
    ` : ''}

    <!-- Token/Cost by Agent -->
    ${Object.keys(data.stats.byAgent || {}).length > 0 ? `
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Usage by Agent</h3>
      <div style="margin-top: var(--space-4)">
        ${Object.entries(data.stats.byAgent).map(([agent, stats]) => `
          <div style="margin-bottom: var(--space-4)">
            <div class="flex justify-between" style="margin-bottom: var(--space-1)">
              <strong>${agent}</strong>
              <span class="text-muted">${stats.messages} msgs | ${stats.tokens.toLocaleString()} tokens | $${stats.cost?.toFixed(4)}</span>
            </div>
            <div class="progress-bar">
              <div class="progress-fill success" style="width: ${Math.min(100, (stats.tokens / data.stats.totalTokens) * 100)}%"></div>
            </div>
          </div>
        `).join('')}
      </div>
    </div>
    ` : ''}
  `;

  // Attach form handler
  const form = document.getElementById('exportTrajectoryForm');
  if (form) {
    form.addEventListener('submit', handleExportTrajectory);
  }
}

async function handleExportTrajectory(e) {
  e.preventDefault();
  const form = e.target;
  const formData = new FormData(form);

  const sessionId = formData.get('sessionId');
  const format = formData.get('format');
  const gzip = formData.get('gzip') === 'on';

  if (!sessionId) {
    Components.toast('Please select a session', 'error');
    return;
  }

  const btn = document.getElementById('exportBtn');
  if (btn) {
    btn.disabled = true;
    /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
    btn.innerHTML = Components.spinner() + ' Exporting...';
  }

  try {
    const result = await API.post('/api/trajectory/export', {
      sessionId,
      format,
      gzip,
    });

    Components.toast('Export completed', 'success');

    // Trigger download
    if (result.downloadUrl) {
      downloadFile(result.downloadUrl, `trajectory-${sessionId.slice(0, 8)}.${format}${gzip ? '.gz' : ''}`);
    }

    loadTrajectoryData();
  } catch (err) {
    Components.toast('Export failed', 'error');
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = 'Export';
    }
  }
}

function selectSession(sessionId) {
  const select = document.getElementById('sessionSelect');
  if (select) {
    select.value = sessionId;
  }
}

async function exportSession(sessionId) {
  try {
    const result = await API.post('/api/trajectory/export', {
      sessionId,
      format: 'jsonl',
      gzip: false,
    });

    Components.toast('Export completed', 'success');

    if (result.downloadUrl) {
      downloadFile(result.downloadUrl, `trajectory-${sessionId.slice(0, 8)}.jsonl`);
    }
  } catch (err) {
    Components.toast('Export failed', 'error');
  }
}

async function exportAllSessions() {
  Components.modal({
    title: 'Export All Sessions',
    content: `
      <p>This will export all sessions. This may take a while for large datasets.</p>
      <div class="form-group" style="margin-top: var(--space-4)">
        <label class="form-label">Format</label>
        <select class="input select" id="exportAllFormat">
          <option value="jsonl">JSONL (JSON Lines)</option>
          <option value="json">JSON (formatted)</option>
        </select>
      </div>
      <div class="form-group">
        <label class="toggle">
          <input type="checkbox" id="exportAllGzip">
          <span class="toggle-slider"></span>
        </label>
        <span style="margin-left: var(--space-2)">Compress (gzip)</span>
      </div>
    `,
    actions: [
      { label: 'Cancel', class: 'btn-secondary', action: 'close' },
      { label: 'Export All', class: 'btn-primary', action: 'doExportAll' },
    ],
  });

  window.doExportAll = async () => {
    const format = document.getElementById('exportAllFormat').value;
    const gzip = document.getElementById('exportAllGzip').checked;

    Components.closeModal();
    Components.toast('Starting export...', 'info');

    try {
      const result = await API.post('/api/trajectory/export-all', { format, gzip });
      Components.toast('Export completed', 'success');

      if (result.downloadUrl) {
        downloadFile(result.downloadUrl, `all-trajectories.${format}${gzip ? '.gz' : ''}`);
      }

      loadTrajectoryData();
    } catch (err) {
      Components.toast('Export failed', 'error');
    }
  };
}

async function downloadExport(exportId) {
  try {
    const result = await API.get(`/api/trajectory/download/${exportId}`);
    if (result.downloadUrl) {
      downloadFile(result.downloadUrl, `export-${exportId}`);
    }
  } catch (err) {
    Components.toast('Download failed', 'error');
  }
}

function downloadFile(url, filename) {
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

function formatAge(timestamp) {
  if (!timestamp) return '—';
  const diff = Date.now() - timestamp;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function formatFileSize(bytes) {
  if (!bytes) return '—';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function getMockTrajectoryData() {
  return {
    sessions: [
      {
        id: 'session-abc123',
        title: 'Main Session',
        messageCount: 847,
        tokens: 1245000,
        cost: 2.47,
        updatedAt: Date.now() - 30 * 60 * 1000,
      },
      {
        id: 'session-def456',
        title: 'Research Session',
        messageCount: 234,
        tokens: 456000,
        cost: 0.91,
        updatedAt: Date.now() - 2 * 60 * 60 * 1000,
      },
      {
        id: 'session-ghi789',
        title: 'Coding Session',
        messageCount: 512,
        tokens: 890000,
        cost: 1.78,
        updatedAt: Date.now() - 24 * 60 * 60 * 1000,
      },
    ],
    stats: {
      totalSessions: 47,
      totalMessages: 28456,
      totalTokens: 45230000,
      totalCost: 89.34,
      byAgent: {
        'coder': { messages: 12400, tokens: 19800000, cost: 39.60 },
        'researcher': { messages: 8900, tokens: 14200000, cost: 28.40 },
        'foreman': { messages: 7156, tokens: 11230000, cost: 22.46 },
      },
    },
    exportHistory: [
      {
        id: 'exp-001',
        filename: 'trajectory-abc123.jsonl',
        format: 'jsonl',
        size: 245760,
        sessionCount: 1,
        timestamp: Date.now() - 60 * 60 * 1000,
      },
      {
        id: 'exp-002',
        filename: 'all-trajectories-2024-01-15.jsonl.gz',
        format: 'jsonl',
        size: 1572864,
        sessionCount: 45,
        timestamp: Date.now() - 24 * 60 * 60 * 1000,
      },
    ],
  };
}

// Expose functions to global scope
window.selectSession = selectSession;
window.exportSession = exportSession;
window.exportAllSessions = exportAllSessions;
window.downloadExport = downloadExport;

// Trajectory page initialization
document.addEventListener('DOMContentLoaded', () => {
  if (document.getElementById('page-trajectory')) {
    loadTrajectoryData();
  }
});
