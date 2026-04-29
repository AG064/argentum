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
  runtimeMode: 'desktop',
  llmProvider: 'openai',
  channelMode: 'local-only',
  securityProfile: 'restricted',
  setupComplete: false,
  setupStatus: 'setup_pending',
  savedConfigPath: '',
};

const setupLabels = {
  runtimeMode: {
    desktop: 'Desktop app with CLI tools',
    cli: 'CLI focused install',
    service: 'Local service with desktop control',
  },
  llmProvider: {
    openai: 'OpenAI',
    gemini: 'Google Gemini',
    anthropic: 'Anthropic',
    local: 'Local model endpoint',
  },
  channelMode: {
    'local-only': 'Local app only',
    'webchat': 'Local webchat',
    'telegram': 'Telegram with allowlist',
  },
  securityProfile: {
    restricted: 'Restricted: workspace-only',
    ask: 'Ask every time',
    session: 'Session grants',
    trusted: 'Trusted mode',
  },
};

const nav = document.querySelector('#section-nav');
const title = document.querySelector('#section-title');
const eyebrow = document.querySelector('#eyebrow');
const viewRoot = document.querySelector('#view-root');
const workspacePath = document.querySelector('#workspace-path');

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => {
    const entities = {
      '&': '&amp;',
      '<': '&lt;',
      '>': '&gt;',
      '"': '&quot;',
      "'": '&#39;',
    };
    return entities[character];
  });
}

function labelFor(group, value) {
  return setupLabels[group][value] || value;
}

function selected(currentValue, optionValue) {
  return currentValue === optionValue ? 'selected' : '';
}

function bindSelect(selector, update) {
  const select = document.querySelector(selector);
  if (select) {
    select.addEventListener('change', (event) => {
      update(event.target.value);
      render();
    });
  }
}

function buildSetupPayload() {
  return {
    workspacePath: state.workspacePath,
    runtimeMode: state.runtimeMode,
    llmProvider: state.llmProvider,
    channelMode: state.channelMode,
    securityProfile: state.securityProfile,
    version: '0.0.4',
  };
}

async function saveSetup() {
  const invoke = window.__TAURI__?.core?.invoke;
  const request = buildSetupPayload();

  if (!invoke) {
    return {
      status: 'setup_saved',
      configPath: `${request.workspacePath}\\config\\default.yaml`,
    };
  }

  return invoke('save_setup', { request });
}

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
  const backButton = { disabled: state.onboardingStep === 1 };
  const isFinalStep = state.onboardingStep === onboardingSteps.length;
  const currentStep = onboardingSteps[state.onboardingStep - 1];

  title.textContent = currentStep;
  eyebrow.textContent = 'Onboarding';
  viewRoot.innerHTML = `
    <div class="onboarding-layout">
      <section class="panel">
        <div class="panel-header">
          <h2>${currentStep}</h2>
          <p>Argentum starts in a restricted workspace and expands only when you approve a capability.</p>
        </div>
        <div class="panel-body">
          ${renderOnboardingStep()}
          <div class="button-row">
            <button class="button" id="back-button" ${backButton.disabled ? 'disabled' : ''}>Back</button>
            <button class="button primary" id="next-button">${isFinalStep ? 'Launch Argentum' : 'Next'}</button>
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

  const input = document.querySelector('#workspace-input');
  if (input) {
    input.addEventListener('input', (event) => {
      state.workspacePath = event.target.value;
      workspacePath.textContent = state.workspacePath;
    });
  }

  bindSelect('#runtime-mode', (value) => {
    state.runtimeMode = value;
  });
  bindSelect('#llm-provider', (value) => {
    state.llmProvider = value;
  });
  bindSelect('#channel-mode', (value) => {
    state.channelMode = value;
  });
  bindSelect('#security-profile', (value) => {
    state.securityProfile = value;
  });

  document.querySelector('#next-button').addEventListener('click', async () => {
    if (state.onboardingStep === onboardingSteps.length) {
      try {
        const result = await saveSetup();
        state.setupComplete = true;
        state.setupStatus = 'setup_saved';
        state.savedConfigPath = result.configPath || result.config_path || '';
        state.activeSection = 'chat';
      } catch (error) {
        state.setupStatus = 'setup-error';
        state.savedConfigPath = error instanceof Error ? error.message : String(error);
      }
      render();
      return;
    }

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
              <input id="workspace-input" value="${escapeHtml(state.workspacePath)}" />
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
          Provider
          <select id="llm-provider">
            <option value="openai" ${selected(state.llmProvider, 'openai')}>OpenAI</option>
            <option value="gemini" ${selected(state.llmProvider, 'gemini')}>Google Gemini</option>
            <option value="anthropic" ${selected(state.llmProvider, 'anthropic')}>Anthropic</option>
            <option value="local" ${selected(state.llmProvider, 'local')}>Local model endpoint</option>
          </select>
        </label>
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

  if (state.onboardingStep === 3) {
    return `
      <div class="form-grid">
        <label>
          Runtime mode
          <select id="runtime-mode">
            <option value="desktop" ${selected(state.runtimeMode, 'desktop')}>Desktop app with CLI tools</option>
            <option value="cli" ${selected(state.runtimeMode, 'cli')}>CLI focused install</option>
            <option value="service" ${selected(state.runtimeMode, 'service')}>Local service with desktop control</option>
          </select>
        </label>
        <label>
          Startup behavior
          <select>
            <option>Launch desktop interface after setup</option>
            <option>Start minimized and keep local services off until requested</option>
          </select>
        </label>
      </div>
    `;
  }

  if (state.onboardingStep === 6) {
    return `
      <div class="form-grid">
        <label>
          Channels
          <select id="channel-mode">
            <option value="local-only" ${selected(state.channelMode, 'local-only')}>Local app only</option>
            <option value="webchat" ${selected(state.channelMode, 'webchat')}>Local webchat</option>
            <option value="telegram" ${selected(state.channelMode, 'telegram')}>Telegram with allowlist</option>
          </select>
        </label>
        <label>
          Webchat token
          <input type="password" placeholder="Generated if local webchat is enabled" />
        </label>
      </div>
    `;
  }

  if (state.onboardingStep === 7) {
    return `
      <div class="form-grid">
        <label>
          Permission profile
          <select id="security-profile">
            <option value="restricted" ${selected(state.securityProfile, 'restricted')}>Restricted: workspace-only</option>
            <option value="ask" ${selected(state.securityProfile, 'ask')}>Ask every time</option>
            <option value="session" ${selected(state.securityProfile, 'session')}>Session grants</option>
            <option value="trusted" ${selected(state.securityProfile, 'trusted')}>Trusted mode</option>
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
      ${renderReviewRows()}
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

function renderReviewRows() {
  return `
    <div class="status-list">
      <div class="status-row"><strong>Workspace</strong><span>${escapeHtml(state.workspacePath)}</span><span class="pill ok">Ready</span></div>
      <div class="status-row"><strong>Runtime</strong><span>${escapeHtml(labelFor('runtimeMode', state.runtimeMode))}</span><span class="pill ok">Ready</span></div>
      <div class="status-row"><strong>Provider</strong><span>${escapeHtml(labelFor('llmProvider', state.llmProvider))}</span><span class="pill warn">Needs key</span></div>
      <div class="status-row"><strong>Channels</strong><span>${escapeHtml(labelFor('channelMode', state.channelMode))}</span><span class="pill ok">Ready</span></div>
      <div class="status-row"><strong>Security</strong><span>${escapeHtml(labelFor('securityProfile', state.securityProfile))}, capability broker active</span><span class="pill ok">Ready</span></div>
      <div class="status-row"><strong>Config</strong><span>Will be saved under config/default.yaml and secrets.env.</span><span class="pill warn">Pending</span></div>
    </div>
  `;
}

function renderSection() {
  const section = sections.find((item) => item.id === state.activeSection) || sections[0];
  title.textContent = section.title;
  eyebrow.textContent = state.setupComplete ? 'Ready' : 'Argentum';
  viewRoot.innerHTML = `
    <div class="dashboard-grid">
      <div class="metric"><span>Status</span><strong>${state.setupComplete ? 'Ready' : 'Local'}</strong></div>
      <div class="metric"><span>Provider</span><strong>${escapeHtml(labelFor('llmProvider', state.llmProvider))}</strong></div>
      <div class="metric"><span>Permission profile</span><strong>${escapeHtml(labelFor('securityProfile', state.securityProfile))}</strong></div>
      <div class="metric"><span>Audit entries</span><strong>0</strong></div>
    </div>
    <div class="status-list">
      <div class="status-row"><strong>Setup</strong><span>${escapeHtml(state.savedConfigPath || 'Workspace setup is ready to save.')}</span><span class="pill ${state.setupStatus === 'setup-error' ? 'warn' : 'ok'}">${state.setupStatus === 'setup-error' ? 'Error' : 'Saved'}</span></div>
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
