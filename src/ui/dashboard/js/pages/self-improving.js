/**
 * Argentum Dashboard - Self-Improving Page
 */

async function loadSelfImprovingData() {
  const container = document.getElementById('selfImprovingContainer');
  if (!container) return;

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  container.innerHTML = `
    <div class="flex justify-center items-center" style="height: 200px">
      ${Components.spinner('lg')}
    </div>
  `;

  try {
    const response = await API.get('/api/self-improving');
    renderSelfImprovingPage(response);
  } catch (err) {
    renderSelfImprovingPage(getMockSelfImprovingData());
  }
}

function renderSelfImprovingPage(data) {
  const container = document.getElementById('selfImprovingContainer');
  if (!container) return;

  const lastRunAgo = data.lastRunTime ? formatAge(data.lastRunTime) : 'Never';
  const nextRunIn = data.nextScheduledRun ? `in ${formatDuration(data.nextScheduledRun - Date.now())}` : 'Not scheduled';

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  container.innerHTML = `
    <!-- Status Card -->
    <div class="card">
      <div class="card-header">
        <h3 class="card-title">Self-Improving Loop</h3>
        <label class="toggle">
          <input type="checkbox" id="selfImprovingToggle" ${data.enabled ? 'checked' : ''} 
                 onchange="toggleSelfImproving(this.checked)">
          <span class="toggle-slider"></span>
        </label>
      </div>
      <div style="margin-top: var(--space-4)">
        <div class="flex gap-6" style="flex-wrap: wrap">
          <div>
            <div class="text-sm text-muted">Last Run</div>
            <div class="stat-card-value" style="font-size: var(--font-size-lg)">${lastRunAgo}</div>
          </div>
          <div>
            <div class="text-sm text-muted">Next Scheduled</div>
            <div class="stat-card-value" style="font-size: var(--font-size-lg)">${nextRunIn}</div>
          </div>
          <div>
            <div class="text-sm text-muted">Skills Created</div>
            <div class="stat-card-value" style="font-size: var(--font-size-lg)">${data.skillsCreated || 0}</div>
          </div>
          <div>
            <div class="text-sm text-muted">Lessons Learned</div>
            <div class="stat-card-value" style="font-size: var(--font-size-lg)">${data.lessonsLearned || 0}</div>
          </div>
        </div>
        <div style="margin-top: var(--space-4)">
          <button class="btn btn-primary" onclick="triggerSelfImproving()">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 16px; height: 16px">
              <polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/>
              <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>
            </svg>
            Run Now
          </button>
          <button class="btn btn-secondary" onclick="loadSelfImprovingData()">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 16px; height: 16px">
              <polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/>
              <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>
            </svg>
            Refresh
          </button>
        </div>
      </div>
    </div>

    <!-- Configuration -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Configuration</h3>
      <form id="selfImprovingConfigForm" style="margin-top: var(--space-4)">
        <div class="form-grid">
          <div class="form-group">
            <label class="form-label">Schedule</label>
            <select class="input select" name="schedule">
              <option value="nightly" ${data.config.schedule === 'nightly' ? 'selected' : ''}>Nightly (03:00)</option>
              <option value="hourly" ${data.config.schedule === 'hourly' ? 'selected' : ''}>Hourly</option>
              <option value="manual" ${data.config.schedule === 'manual' ? 'selected' : ''}>Manual Only</option>
              <option value="idle" ${data.config.schedule === 'idle' ? 'selected' : ''}>When Idle</option>
            </select>
          </div>
          <div class="form-group">
            <label class="form-label">Idle Threshold (minutes)</label>
            <input type="number" class="input" name="idleThreshold" 
                   value="${data.config.idleThreshold || 120}" min="5" max="480">
          </div>
          <div class="form-group">
            <label class="form-label">Skill Creation Threshold</label>
            <input type="number" class="input" name="skillThreshold" 
                   value="${data.config.skillCreationThreshold || 5}" min="1" max="20">
            <small class="form-hint">Complexity × frequency score required before creating a skill</small>
          </div>
          <div class="form-group">
            <label class="form-label">Max Skills Per Run</label>
            <input type="number" class="input" name="maxSkills" 
                   value="${data.config.maxSkillsPerRun || 3}" min="1" max="10">
          </div>
        </div>
        <div style="margin-top: var(--space-4)">
          <button type="submit" class="btn btn-primary">Save Configuration</button>
        </div>
      </form>
    </div>

    <!-- Run History -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Recent Runs</h3>
      ${data.runHistory && data.runHistory.length > 0 ? `
        <div class="table-container" style="margin-top: var(--space-4)">
          <table class="table">
            <thead>
              <tr>
                <th>Time</th>
                <th>Duration</th>
                <th>Phases</th>
                <th>Skills</th>
                <th>Lessons</th>
                <th>Corrections</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              ${data.runHistory.map(run => `
                <tr>
                  <td>${new Date(run.timestamp).toLocaleString()}</td>
                  <td>${(run.duration / 1000).toFixed(1)}s</td>
                  <td>${run.phasesCompleted || 0}</td>
                  <td><span class="badge badge-success">+${run.skillsCreated || 0}</span></td>
                  <td><span class="badge badge-info">${run.lessonsLearned || 0}</span></td>
                  <td><span class="badge badge-warning">${run.correctionsApplied || 0}</span></td>
                  <td>
                    <span class="badge ${run.success ? 'badge-success' : 'badge-danger'}">
                      ${run.success ? 'Success' : 'Failed'}
                    </span>
                  </td>
                </tr>
              `).join('')}
            </tbody>
          </table>
        </div>
      ` : `
        <div style="margin-top: var(--space-4); padding: var(--space-6); text-align: center; color: var(--color-text-muted)">
          No improvement runs yet. Click "Run Now" to start.
        </div>
      `}
    </div>

    <!-- Lessons Learned Log -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Lessons Learned</h3>
      ${data.lessons && data.lessons.length > 0 ? `
        <div style="margin-top: var(--space-4)">
          ${data.lessons.map(lesson => `
            <div class="lesson-item" style="margin-bottom: var(--space-4); padding-bottom: var(--space-4); border-bottom: 1px solid var(--color-border)">
              <div class="flex items-start gap-3">
                <span style="font-size: var(--font-size-xl)">${getLessonIcon(lesson.category)}</span>
                <div style="flex: 1">
                  <div class="flex items-center gap-2" style="margin-bottom: var(--space-1)">
                    <strong>${lesson.title}</strong>
                    <span class="badge badge-${getLessonBadgeClass(lesson.category)}">${lesson.category.replace('_', ' ')}</span>
                  </div>
                  <p style="color: var(--color-text-muted); margin-bottom: var(--space-2)">${lesson.description || ''}</p>
                  <div class="text-sm text-muted">${formatAge(lesson.timestamp)}</div>
                </div>
              </div>
            </div>
          `).join('')}
        </div>
      ` : `
        <div style="margin-top: var(--space-4); padding: var(--space-6); text-align: center; color: var(--color-text-muted)">
          No lessons learned yet. Lessons are automatically captured during agent interactions.
        </div>
      `}
    </div>

    <!-- Phase Status -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Improvement Phases</h3>
      <div style="margin-top: var(--space-4); display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: var(--space-4)">
        ${['error', 'skill', 'memory', 'model', 'correction'].map(phase => {
          const phaseData = data.phases?.[phase] || { enabled: true, lastRun: null, status: 'pending' };
          return `
            <div class="phase-card" style="padding: var(--space-4); border: 1px solid var(--color-border); border-radius: var(--radius-md)">
              <div class="flex items-center gap-2" style="margin-bottom: var(--space-2)">
                <span style="font-size: var(--font-size-xl)">${getPhaseIcon(phase)}</span>
                <strong>${phase.charAt(0).toUpperCase() + phase.slice(1)}</strong>
              </div>
              <div class="text-sm text-muted" style="margin-bottom: var(--space-2)">
                ${getPhaseDescription(phase)}
              </div>
              <div class="text-sm">
                Status: <span class="badge badge-${phaseData.status === 'success' ? 'success' : phaseData.status === 'failed' ? 'danger' : 'muted'}">${phaseData.status || 'pending'}</span>
              </div>
              ${phaseData.lastRun ? `
                <div class="text-sm text-muted">Last: ${formatAge(phaseData.lastRun)}</div>
              ` : ''}
            </div>
          `;
        }).join('')}
      </div>
    </div>
  `;

  // Attach form handler
  const form = document.getElementById('selfImprovingConfigForm');
  if (form) {
    form.addEventListener('submit', handleSelfImprovingConfigSubmit);
  }
}

function getLessonIcon(category) {
  const icons = {
    insight: '💡',
    mistake: '🔴',
    pattern: '📈',
    skill_created: '🛠️',
    knowledge_gap: '❓',
    correction: '✅',
  };
  return icons[category] || '📝';
}

function getLessonBadgeClass(category) {
  const classes = {
    insight: 'info',
    mistake: 'danger',
    pattern: 'warning',
    skill_created: 'success',
    knowledge_gap: 'muted',
    correction: 'success',
  };
  return classes[category] || 'muted';
}

function getPhaseIcon(phase) {
  const icons = {
    error: '🔍',
    skill: '🛠️',
    memory: '🧠',
    model: '🤖',
    correction: '✅',
  };
  return icons[phase] || '⚙️';
}

function getPhaseDescription(phase) {
  const descriptions = {
    error: 'Analyze error logs and fix recurring issues',
    skill: 'Identify patterns and create new skills',
    memory: 'Optimize memory storage and retrieval',
    model: 'Adjust model parameters and prompts',
    correction: 'Apply user corrections and feedback',
  };
  return descriptions[phase] || '';
}

function formatAge(timestamp) {
  const diff = Date.now() - timestamp;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function formatDuration(ms) {
  if (ms < 0) return 'now';
  const mins = Math.floor(ms / 60000);
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ${mins % 60}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

async function handleSelfImprovingConfigSubmit(e) {
  e.preventDefault();
  const form = e.target;
  const formData = new FormData(form);

  const config = {
    schedule: formData.get('schedule'),
    idleThreshold: parseInt(formData.get('idleThreshold')),
    skillCreationThreshold: parseInt(formData.get('skillThreshold')),
    maxSkillsPerRun: parseInt(formData.get('maxSkills')),
  };

  try {
    await API.post('/api/self-improving/config', config);
    Components.toast('Configuration saved', 'success');
  } catch (err) {
    Components.toast('Failed to save configuration', 'error');
  }
}

async function toggleSelfImproving(enabled) {
  try {
    await API.post('/api/self-improving/toggle', { enabled });
    Components.toast(`Self-improving ${enabled ? 'enabled' : 'disabled'}`, 'success');
  } catch (err) {
    Components.toast('Failed to toggle self-improving', 'error');
    document.getElementById('selfImprovingToggle').checked = !enabled;
  }
}

async function triggerSelfImproving() {
  Components.toast('Starting self-improving loop...', 'info');
  try {
    const result = await API.post('/api/self-improving/run', { force: true });
    Components.toast('Self-improving loop completed', 'success');
    loadSelfImprovingData();
  } catch (err) {
    Components.toast('Failed to run self-improving loop', 'error');
  }
}

function getMockSelfImprovingData() {
  return {
    enabled: true,
    lastRunTime: Date.now() - 3 * 60 * 60 * 1000, // 3 hours ago
    nextScheduledRun: Date.now() + 21 * 60 * 60 * 1000, // in 21 hours
    skillsCreated: 2,
    lessonsLearned: 12,
    config: {
      schedule: 'nightly',
      idleThreshold: 120,
      skillCreationThreshold: 5,
      maxSkillsPerRun: 3,
    },
    phases: {
      error: { enabled: true, lastRun: Date.now() - 3 * 60 * 60 * 1000, status: 'success' },
      skill: { enabled: true, lastRun: Date.now() - 3 * 60 * 60 * 1000, status: 'success' },
      memory: { enabled: true, lastRun: Date.now() - 3 * 60 * 60 * 1000, status: 'success' },
      model: { enabled: true, lastRun: Date.now() - 3 * 60 * 60 * 1000, status: 'success' },
      correction: { enabled: true, lastRun: Date.now() - 3 * 60 * 60 * 1000, status: 'success' },
    },
    runHistory: [
      {
        timestamp: Date.now() - 3 * 60 * 60 * 1000,
        duration: 45000,
        phasesCompleted: 5,
        skillsCreated: 1,
        lessonsLearned: 3,
        correctionsApplied: 1,
        success: true,
      },
      {
        timestamp: Date.now() - 27 * 60 * 60 * 1000,
        duration: 62000,
        phasesCompleted: 5,
        skillsCreated: 1,
        lessonsLearned: 5,
        correctionsApplied: 2,
        success: true,
      },
      {
        timestamp: Date.now() - 51 * 60 * 60 * 1000,
        duration: 38000,
        phasesCompleted: 5,
        skillsCreated: 0,
        lessonsLearned: 4,
        correctionsApplied: 0,
        success: true,
      },
    ],
    lessons: [
      {
        id: '1',
        category: 'mistake',
        title: 'Failed to handle rate limits gracefully',
        description: 'When the LLM provider returned 429, the agent crashed instead of retrying with backoff.',
        timestamp: Date.now() - 2 * 60 * 60 * 1000,
      },
      {
        id: '2',
        category: 'pattern',
        title: 'Users frequently ask about weather',
        description: 'Detected 15+ queries about weather in the past week. Should create a weather skill.',
        timestamp: Date.now() - 5 * 60 * 60 * 1000,
      },
      {
        id: '3',
        category: 'skill_created',
        title: 'Created weather-skill',
        description: 'Automatically generated skill to query wttr.in for weather information.',
        timestamp: Date.now() - 8 * 60 * 60 * 1000,
      },
      {
        id: '4',
        category: 'knowledge_gap',
        title: 'Limited knowledge of local events',
        description: 'Agent struggles with queries about local events. Need to integrate events API.',
        timestamp: Date.now() - 12 * 60 * 60 * 1000,
      },
      {
        id: '5',
        category: 'insight',
        title: 'Structured output is 40% faster',
        description: 'Switching from free-form to JSON mode for certain tasks improved token efficiency.',
        timestamp: Date.now() - 24 * 60 * 60 * 1000,
      },
    ],
  };
}

// Expose functions to global scope
window.toggleSelfImproving = toggleSelfImproving;
window.triggerSelfImproving = triggerSelfImproving;

// Self-improving page initialization
document.addEventListener('DOMContentLoaded', () => {
  if (document.getElementById('page-self-improving')) {
    loadSelfImprovingData();
  }
});
