const sections = [
  { id: 'onboarding', icon: 'ON', title: 'Onboarding' },
  { id: 'chat', icon: 'CH', title: 'Chat' },
  { id: 'agents', icon: 'AG', title: 'Agents' },
  { id: 'runner', icon: 'RN', title: 'Agent Runner' },
  { id: 'skills', icon: 'SK', title: 'Skills Library' },
  { id: 'webchat', icon: 'WC', title: 'Webchat Settings' },
  { id: 'graph', icon: 'KG', title: 'Knowledge Graph' },
  { id: 'memory', icon: 'MM', title: 'Memory' },
  { id: 'logs', icon: 'LG', title: 'Activity Logs' },
  { id: 'security', icon: 'SC', title: 'Security & Permissions' },
  { id: 'settings', icon: 'ST', title: 'Settings' },
  { id: 'diagnostics', icon: 'DX', title: 'Diagnostics' },
];

const onboardingSteps = [
  'Welcome',
  'Workspace and data location',
  'Runtime mode',
  'LLM provider',
  'Secrets and credentials',
  'Channels and webchat',
  'Security posture',
  'Review',
  'Finish and launch',
];

const state = {
  activeSection: 'onboarding',
  onboardingStep: 1,
  workspacePath: '%LOCALAPPDATA%\\Programs\\Argentum\\workspace',
};

const nav = document.querySelector('#section-nav');
const title = document.querySelector('#section-title');
const eyebrow = document.querySelector('#eyebrow');
const viewRoot = document.querySelector('#view-root');
const workspacePath = document.querySelector('#workspace-path');

function renderNavigation() {
  nav.innerHTML = sections
    .map(
      (section) => `
        <button class="nav-button ${section.id === state.activeSection ? 'active' : ''}" data-section="${section.id}">
          <span class="nav-icon">${section.icon}</span>
          <span>${section.title}</span>
        </button>
      `,
    )
    .join('');

  nav.querySelectorAll('[data-section]').forEach((button) => {
    button.addEventListener('click', () => {
      state.activeSection = button.dataset.section;
      render();
    });
  });
}

function renderOnboarding() {
  title.textContent = onboardingSteps[state.onboardingStep - 1];
  eyebrow.textContent = 'Onboarding';
  viewRoot.innerHTML = `
    <div class="onboarding-layout">
      <section class="panel">
        <div class="panel-header">
          <h2>${onboardingSteps[state.onboardingStep - 1]}</h2>
          <p>Argentum starts in a restricted workspace and expands only when you approve a capability.</p>
        </div>
        <div class="panel-body">
          ${renderOnboardingStep()}
          <div class="button-row">
            <button class="button" id="back-button">Back</button>
            <button class="button primary" id="next-button">Next</button>
          </div>
        </div>
      </section>
      <aside class="panel">
        <div class="panel-header">
          <h3>Setup Flow</h3>
        </div>
        <div class="panel-body step-list">
          ${onboardingSteps
            .map(
              (step, index) => `
                <div class="step-card ${index + 1 === state.onboardingStep ? 'active' : ''}">
                  <span>Step ${index + 1}</span>
                  <strong>${step}</strong>
                </div>
              `,
            )
            .join('')}
        </div>
      </aside>
    </div>
  `;

  document.querySelector('#back-button').addEventListener('click', () => {
    state.onboardingStep = Math.max(1, state.onboardingStep - 1);
    render();
  });
  document.querySelector('#next-button').addEventListener('click', () => {
    state.onboardingStep = Math.min(onboardingSteps.length, state.onboardingStep + 1);
    render();
  });
}

function renderOnboardingStep() {
  if (state.onboardingStep === 2) {
    return `
      <div class="form-grid">
        <label>
          Workspace
          <input id="workspace-input" value="${state.workspacePath}" />
        </label>
        <label>
          Data mode
          <select>
            <option>Store config, data, secrets, logs, and cache under the selected workspace</option>
          </select>
        </label>
      </div>
    `;
  }

  if (state.onboardingStep === 5) {
    return `
      <div class="form-grid">
        <label>
          Provider key
          <input type="password" placeholder="Stored in secrets.env, never YAML" />
        </label>
        <label>
          Webchat token
          <input type="password" placeholder="Generated if left empty" />
        </label>
      </div>
    `;
  }

  if (state.onboardingStep === 7) {
    return `
      <div class="form-grid">
        <label>
          Permission profile
          <select>
            <option>Restricted: workspace-only</option>
            <option>Ask Every Time</option>
            <option>Session Grant</option>
            <option>Trusted Mode</option>
          </select>
        </label>
        <label>
          Audit file
          <input value="data\\audit\\capabilities.log" />
        </label>
      </div>
    `;
  }

  if (state.onboardingStep === 8) {
    return `
      <div class="status-list">
        <div class="status-row"><strong>Workspace</strong><span>${state.workspacePath}</span><span class="pill ok">Ready</span></div>
        <div class="status-row"><strong>Security</strong><span>Restricted profile, capability broker active</span><span class="pill ok">Ready</span></div>
        <div class="status-row"><strong>Secrets</strong><span>Masked inputs, dotenv output</span><span class="pill warn">Review</span></div>
      </div>
    `;
  }

  return `
    <div class="dashboard-grid">
      <div class="metric"><span>Default access</span><strong>Workspace</strong></div>
      <div class="metric"><span>Privileged actions</span><strong>Ask</strong></div>
      <div class="metric"><span>Audit mode</span><strong>On</strong></div>
    </div>
  `;
}

function renderSection() {
  const section = sections.find((item) => item.id === state.activeSection) || sections[0];
  title.textContent = section.title;
  eyebrow.textContent = 'Argentum';
  viewRoot.innerHTML = `
    <div class="dashboard-grid">
      <div class="metric"><span>Status</span><strong>Local</strong></div>
      <div class="metric"><span>Permission profile</span><strong>Restricted</strong></div>
      <div class="metric"><span>Audit entries</span><strong>0</strong></div>
    </div>
    <div class="status-list">
      <div class="status-row"><strong>${section.title}</strong><span>Desktop shell route is ready for runtime wiring.</span><span class="pill ok">Ready</span></div>
      <div class="status-row"><strong>Broker</strong><span>Capability grants and denials will surface here.</span><span class="pill warn">Next</span></div>
    </div>
  `;
}

function render() {
  workspacePath.textContent = state.workspacePath;
  renderNavigation();
  if (state.activeSection === 'onboarding') renderOnboarding();
  else renderSection();
}

document.querySelector('#audit-button').addEventListener('click', () => {
  state.activeSection = 'security';
  render();
});

document.querySelector('#settings-button').addEventListener('click', () => {
  state.activeSection = 'settings';
  render();
});

render();
