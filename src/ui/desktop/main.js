const sections = [
  { id: 'onboarding', icon: 'ON', title: 'Onboarding', eyebrow: 'First run' },
  { id: 'chat', icon: 'CH', title: 'Chat', eyebrow: 'Workspace' },
  { id: 'agents', icon: 'AG', title: 'Agents', eyebrow: 'Profiles' },
  { id: 'runner', icon: 'RN', title: 'Agent Runner', eyebrow: 'Execution' },
  { id: 'skills', icon: 'SK', title: 'Skills Library', eyebrow: 'Capabilities' },
  { id: 'webchat', icon: 'WC', title: 'Webchat Settings', eyebrow: 'Channels' },
  { id: 'graph', icon: 'KG', title: 'Knowledge Graph', eyebrow: 'Memory map' },
  { id: 'memory', icon: 'MM', title: 'Memory', eyebrow: 'Recall' },
  { id: 'logs', icon: 'LG', title: 'Activity Logs', eyebrow: 'Timeline' },
  { id: 'security', icon: 'SC', title: 'Security & Permissions', eyebrow: 'Control' },
  { id: 'settings', icon: 'ST', title: 'Settings', eyebrow: 'Configuration' },
  { id: 'diagnostics', icon: 'DX', title: 'Diagnostics', eyebrow: 'Doctor' },
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

const commandCatalog = [
  {
    id: 'chat-start',
    section: 'chat',
    title: 'Start local gateway',
    command: 'argentum gateway start',
    summary: 'Bring the local API and web gateway online for chat, agents, and integrations.',
    risk: 'Network: local',
  },
  {
    id: 'gateway-status',
    section: 'chat',
    title: 'Check gateway status',
    command: 'argentum gateway status',
    summary: 'Confirm whether the local gateway is running and which port it owns.',
    risk: 'Read-only',
  },
  {
    id: 'agents-list',
    section: 'agents',
    title: 'List agents',
    command: 'argentum agents list',
    summary: 'Show configured agent profiles, model routing, and workspace scope.',
    risk: 'Read-only',
  },
  {
    id: 'agents-create',
    section: 'agents',
    title: 'Create agent profile',
    command: 'argentum agents create --name "researcher"',
    summary: 'Create a scoped agent profile with its own prompt and model preferences.',
    risk: 'Writes config',
  },
  {
    id: 'runner-acp',
    section: 'runner',
    title: 'Run sandbox task',
    command: 'argentum acp run "code"',
    summary: 'Execute approved code through the sandbox runner, not arbitrary desktop access.',
    risk: 'Approval required',
  },
  {
    id: 'cron-list',
    section: 'runner',
    title: 'View scheduled jobs',
    command: 'argentum cron list',
    summary: 'Inspect scheduled jobs before enabling, disabling, or running them.',
    risk: 'Read-only',
  },
  {
    id: 'skills-list',
    section: 'skills',
    title: 'List installed skills',
    command: 'argentum skill list',
    summary: 'Audit installed skills and their workspace-visible capabilities.',
    risk: 'Read-only',
  },
  {
    id: 'skills-search',
    section: 'skills',
    title: 'Search skills',
    command: 'argentum skill search "browser"',
    summary: 'Find installable skills before granting them workspace access.',
    risk: 'Network optional',
  },
  {
    id: 'webchat-config',
    section: 'webchat',
    title: 'Show webchat config',
    command: 'argentum config features.webchat',
    summary: 'Review token, port, and allowed origin settings for local webchat.',
    risk: 'Read-only',
  },
  {
    id: 'telegram-status',
    section: 'webchat',
    title: 'Telegram status',
    command: 'argentum telegram status',
    summary: 'Check Telegram allowlist and bot status without exposing the token.',
    risk: 'Read-only',
  },
  {
    id: 'graph-feature',
    section: 'graph',
    title: 'Inspect graph feature',
    command: 'argentum feature knowledge-graph',
    summary: 'View feature health, configuration, and entry points for graph memory.',
    risk: 'Read-only',
  },
  {
    id: 'memory-search',
    section: 'memory',
    title: 'Search memory',
    command: 'argentum memory search "project context"',
    summary: 'Search workspace memory without granting access outside the Argentum folder.',
    risk: 'Read-only',
  },
  {
    id: 'memory-list',
    section: 'memory',
    title: 'List memory namespaces',
    command: 'argentum memory list',
    summary: 'Inspect stored memory namespaces before export, import, or purge operations.',
    risk: 'Read-only',
  },
  {
    id: 'logs-gateway',
    section: 'logs',
    title: 'View gateway logs',
    command: 'argentum gateway logs --lines 100',
    summary: 'Read recent gateway logs from the selected workspace.',
    risk: 'Read-only',
  },
  {
    id: 'security-status',
    section: 'security',
    title: 'Security overview',
    command: 'argentum security status',
    summary: 'Review policy, sandbox, credentials, approvals, and audit posture.',
    risk: 'Read-only',
  },
  {
    id: 'security-approvals',
    section: 'security',
    title: 'Review approvals',
    command: 'argentum security approvals',
    summary: 'Inspect pending permission requests before granting a scoped capability.',
    risk: 'Approval required',
  },
  {
    id: 'security-audit',
    section: 'security',
    title: 'Open audit log',
    command: 'argentum security audit',
    summary: 'Read capability decisions, denials, and sensitive-action history.',
    risk: 'Read-only',
  },
  {
    id: 'settings-config',
    section: 'settings',
    title: 'Show config',
    command: 'argentum config',
    summary: 'Open current config values and verify where they are stored.',
    risk: 'Read-only',
  },
  {
    id: 'doctor',
    section: 'diagnostics',
    title: 'Run doctor',
    command: 'argentum doctor',
    summary: 'Check Node, config, data folders, compiled features, and native dependencies.',
    risk: 'Read-only',
  },
  {
    id: 'image-generate',
    section: 'skills',
    title: 'Generate image',
    command: 'argentum image "prompt"',
    summary: 'Use the image generation feature through the same provider and secret controls.',
    risk: 'Provider usage',
  },
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
  actionStatus: 'No GUI action has run in this session.',
  copiedCommand: '',
  desktopState: {
    workspaceReady: false,
    configExists: false,
    dataExists: false,
    logsExists: false,
    gatewayPid: null,
    gatewayLogPreview: 'No entries yet.',
    auditLogPreview: 'No entries yet.',
  },
  chatMessages: [
    {
      speaker: 'Argentum',
      text: 'Local workspace is ready. Start the gateway or keep drafting offline.',
    },
    {
      speaker: 'System',
      text: 'Restricted profile is active. File, shell, network, and OS automation require scoped approval.',
    },
  ],
  draftMessage: '',
  pendingApprovals: [
    {
      id: 'workspace-read',
      title: 'Read selected workspace',
      detail: 'Allow agents to read files under the Argentum workspace root.',
      status: 'Allowed by default',
    },
    {
      id: 'shell-run',
      title: 'Run shell command',
      detail: 'Requires explicit command, working directory, and expiration.',
      status: 'Ask every time',
    },
    {
      id: 'network-send',
      title: 'External network request',
      detail: 'Requires destination, provider, and purpose before execution.',
      status: 'Blocked',
    },
  ],
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

function escapeAttribute(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}

function labelFor(group, value) {
  return setupLabels[group][value] || value;
}

function selected(currentValue, optionValue) {
  return currentValue === optionValue ? 'selected' : '';
}

function actionsFor(sectionId) {
  return commandCatalog.filter((action) => action.section === sectionId);
}

function formatWorkspaceHealth() {
  if (!state.desktopState?.workspaceReady) return 'Not created';
  if (state.desktopState.configExists && state.desktopState.dataExists) return 'Ready';
  return 'Needs setup';
}

function formatFound(value) {
  return value ? 'Found' : 'Missing';
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

async function hydrateDesktopDefaults() {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) return;

  try {
    const defaults = await invoke('desktop_defaults');
    if (defaults?.defaultWorkspacePath && state.workspacePath.includes('%LOCALAPPDATA%')) {
      state.workspacePath = defaults.defaultWorkspacePath;
      render();
    }
  } catch (error) {
    state.actionStatus = error instanceof Error ? error.message : String(error);
  }
}

async function refreshDesktopState(options = {}) {
  const { renderAfter = true, announce = false } = options;
  const invoke = window.__TAURI__?.core?.invoke;

  if (!invoke) {
    state.desktopState = {
      workspacePath: state.workspacePath,
      configPath: `${state.workspacePath}\\config\\default.yaml`,
      workspaceReady: Boolean(state.setupComplete),
      configExists: Boolean(state.setupComplete),
      dataExists: Boolean(state.setupComplete),
      logsExists: Boolean(state.setupComplete),
      gatewayPid: null,
      gatewayLogPreview: 'Desktop preview mode. Run the installed app to read local logs.',
      auditLogPreview: 'Desktop preview mode. Run the installed app to read audit history.',
    };
    if (announce) state.actionStatus = 'Workspace state refreshed.';
    if (renderAfter) render();
    return;
  }

  try {
    state.desktopState = await invoke('desktop_state', {
      request: { workspacePath: state.workspacePath },
    });
    if (announce) state.actionStatus = 'Workspace state refreshed.';
  } catch (error) {
    state.actionStatus = error instanceof Error ? error.message : String(error);
  }

  if (renderAfter) render();
}

async function runAction(actionId) {
  const action = commandCatalog.find((item) => item.id === actionId);
  if (!action) return;

  const invoke = window.__TAURI__?.core?.invoke;
  if (invoke) {
    try {
      const result = await invoke('run_desktop_action', {
        request: { actionId, workspacePath: state.workspacePath },
      });
      state.actionStatus = result?.message || `${action.title} completed.`;
    } catch (error) {
      state.actionStatus = error instanceof Error ? error.message : String(error);
    }
  } else {
    state.actionStatus = `${action.title} prepared. CLI equivalent: ${action.command}`;
  }

  await refreshDesktopState({ renderAfter: false });
  render();
}

async function copyCommand(command) {
  state.copiedCommand = command;
  state.actionStatus = `Copied: ${command}`;

  try {
    await navigator.clipboard?.writeText(command);
  } catch (_error) {
    state.actionStatus = `Command ready to copy: ${command}`;
  }

  render();
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
}

function renderActionCards(sectionId, options = {}) {
  const actions = actionsFor(sectionId);
  const items = options.limit ? actions.slice(0, options.limit) : actions;

  return `
    <div class="command-grid">
      ${items
        .map(
          (action) => `
            <article class="command-card">
              <div>
                <span class="pill">${escapeHtml(action.risk)}</span>
                <h3>${escapeHtml(action.title)}</h3>
                <p>${escapeHtml(action.summary)}</p>
              </div>
              <code>${escapeHtml(action.command)}</code>
              <div class="button-row split">
                <button class="button" data-copy-command="${escapeAttribute(action.command)}">Copy</button>
                <button class="button primary" data-run-action="${escapeAttribute(action.id)}">Run</button>
              </div>
            </article>
          `,
        )
        .join('')}
    </div>
  `;
}

function renderHero(kicker, heading, detail, stats = []) {
  return `
    <section class="hero-strip">
      <div>
        <span>${escapeHtml(kicker)}</span>
        <h2>${escapeHtml(heading)}</h2>
        <p>${escapeHtml(detail)}</p>
      </div>
      <div class="hero-stats">
        ${stats
          .map(
            (stat) => `
              <div class="metric compact">
                <span>${escapeHtml(stat.label)}</span>
                <strong>${escapeHtml(stat.value)}</strong>
              </div>
            `,
          )
          .join('')}
      </div>
    </section>
  `;
}

function renderStatusRail() {
  return `
    <aside class="panel status-panel">
      <div class="panel-header">
        <h3>Session State</h3>
      </div>
      <div class="panel-body status-stack">
        <div>
          <span>Workspace</span>
          <strong>${escapeHtml(state.workspacePath)}</strong>
        </div>
        <div>
          <span>Provider</span>
          <strong>${escapeHtml(labelFor('llmProvider', state.llmProvider))}</strong>
        </div>
        <div>
          <span>Security</span>
          <strong>${escapeHtml(labelFor('securityProfile', state.securityProfile))}</strong>
        </div>
        <div>
          <span>Workspace health</span>
          <strong>${escapeHtml(formatWorkspaceHealth())}</strong>
        </div>
        <div>
          <span>Last action</span>
          <strong>${escapeHtml(state.actionStatus)}</strong>
        </div>
      </div>
    </aside>
  `;
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
                <button class="step-card ${index + 1 === state.onboardingStep ? 'active' : ''}" data-onboarding-step="${index + 1}">
                  <span>Step ${index + 1}</span>
                  <strong>${step}</strong>
                </button>
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
        await refreshDesktopState({ renderAfter: false });
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
  if (state.onboardingStep === 1) {
    return `
      <div class="interface-grid">
        <button class="interface-card ${state.runtimeMode === 'desktop' ? 'active' : ''}" data-interface-mode="desktop">
          <span>GUI</span>
          <strong>Desktop control center</strong>
          <p>Onboarding, chat, agents, skills, graph, settings, logs, and approvals in one app.</p>
        </button>
        <button class="interface-card ${state.runtimeMode === 'cli' ? 'active' : ''}" data-interface-mode="cli">
          <span>CLI</span>
          <strong>Terminal-first tools</strong>
          <p>Keep the OpenClaw-style terminal workflow and install the same commands for scripts.</p>
        </button>
        <button class="interface-card ${state.runtimeMode === 'service' ? 'active' : ''}" data-interface-mode="service">
          <span>Service</span>
          <strong>Local service plus GUI</strong>
          <p>Use the desktop app as the control plane while services remain off until approved.</p>
        </button>
      </div>
    `;
  }

  if (state.onboardingStep === 2) {
    return `
      <div class="form-grid">
        <label>
          Workspace
          <input id="workspace-input" value="${escapeAttribute(state.workspacePath)}" />
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
          <input type="password" placeholder="Stored in secrets.env, never YAML" autocomplete="new-password" />
        </label>
        <label>
          Webchat token
          <input type="password" placeholder="Generated if left empty" autocomplete="new-password" />
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
          <input type="password" placeholder="Generated if local webchat is enabled" autocomplete="new-password" />
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
    return renderReviewRows();
  }

  if (state.onboardingStep === 9) {
    return `
      <div class="finish-panel">
        <h3>Ready to enter Argentum</h3>
        <p>Setup will write config under the selected workspace, keep secrets outside YAML, and open the desktop workspace.</p>
        ${renderReviewRows()}
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

function renderChatSection() {
  return `
    ${renderHero(
      'Local chat',
      'Talk to Argentum from the desktop app',
      'Use the GUI for daily work and keep the CLI as the exact terminal equivalent behind every action.',
      [
        { label: 'Provider', value: labelFor('llmProvider', state.llmProvider) },
        {
          label: 'Gateway',
          value: state.desktopState?.gatewayPid
            ? `PID ${state.desktopState.gatewayPid}`
            : 'Stopped',
        },
        { label: 'Mode', value: labelFor('securityProfile', state.securityProfile) },
      ],
    )}
    <div class="workspace-layout">
      <section class="panel chat-shell">
        <div class="panel-header">
          <h3>Conversation</h3>
        </div>
        <div class="chat-transcript">
          ${state.chatMessages
            .map(
              (message) => `
                <div class="message ${message.speaker === 'Argentum' ? 'assistant' : 'system'}">
                  <span>${escapeHtml(message.speaker)}</span>
                  <p>${escapeHtml(message.text)}</p>
                </div>
              `,
            )
            .join('')}
        </div>
        <div class="composer">
          <textarea id="chat-draft" placeholder="Ask Argentum to plan, research, run an agent, or inspect memory."></textarea>
          <button class="button primary" id="send-chat">Send</button>
        </div>
      </section>
      ${renderStatusRail()}
    </div>
    ${renderActionCards('chat')}
  `;
}

function renderAgentsSection() {
  return `
    ${renderHero(
      'Agent profiles',
      'Create focused agents with scoped permissions',
      'Profiles let you keep coding, research, operations, and personal workflows separated while sharing the same secure broker.',
      [
        { label: 'Profiles', value: '3' },
        { label: 'Active', value: 'General' },
        { label: 'Scope', value: 'Workspace' },
      ],
    )}
    <div class="card-grid">
      ${['General operator', 'Research analyst', 'Automation builder']
        .map(
          (agent, index) => `
            <article class="feature-card">
              <span class="pill ${index === 0 ? 'ok' : ''}">${index === 0 ? 'Active' : 'Available'}</span>
              <h3>${agent}</h3>
              <p>${index === 0 ? 'Balanced local assistant for everyday workspace work.' : 'Ready to configure with its own prompt, model, and tool grants.'}</p>
            </article>
          `,
        )
        .join('')}
    </div>
    ${renderActionCards('agents')}
  `;
}

function renderRunnerSection() {
  return `
    ${renderHero(
      'Agent runner',
      'Run work through visible steps and approvals',
      'Every privileged step is surfaced before execution, with sandbox and workspace boundaries kept visible.',
      [
        { label: 'Queue', value: '0' },
        { label: 'Sandbox', value: 'On' },
        { label: 'Approvals', value: 'Manual' },
      ],
    )}
    <section class="panel">
      <div class="panel-header">
        <h3>Run Plan</h3>
      </div>
      <div class="panel-body timeline">
        <div><span>1</span><strong>Understand request</strong><p>Build task context from chat, config, and selected memory.</p></div>
        <div><span>2</span><strong>Request capabilities</strong><p>Ask for file, shell, network, or integration access only when needed.</p></div>
        <div><span>3</span><strong>Execute in scope</strong><p>Use the workspace root and audit log for every action.</p></div>
      </div>
    </section>
    ${renderActionCards('runner')}
  `;
}

function renderSkillsSection() {
  return `
    ${renderHero(
      'Skills library',
      'Install only the capabilities you mean to trust',
      'Skills stay discoverable from the GUI while still mapping back to terminal commands for automation.',
      [
        { label: 'Installed', value: 'Core' },
        { label: 'Updates', value: 'Manual' },
        { label: 'Policy', value: 'Scoped' },
      ],
    )}
    <div class="card-grid">
      ${['Browser automation', 'Image generation', 'Knowledge graph', 'Security audit']
        .map(
          (skill) => `
            <article class="feature-card">
              <span class="pill ok">Available</span>
              <h3>${skill}</h3>
              <p>Uses the selected workspace and asks before crossing a capability boundary.</p>
            </article>
          `,
        )
        .join('')}
    </div>
    ${renderActionCards('skills')}
  `;
}

function renderWebchatSection() {
  return `
    ${renderHero(
      'Channels',
      'Expose Argentum only where you choose',
      'Local app access is the default. Webchat and Telegram remain opt-in with tokens and allowlists.',
      [
        { label: 'Channel mode', value: labelFor('channelMode', state.channelMode) },
        { label: 'Webchat', value: state.channelMode === 'webchat' ? 'On' : 'Off' },
        { label: 'Telegram', value: state.channelMode === 'telegram' ? 'On' : 'Off' },
      ],
    )}
    <section class="panel">
      <div class="panel-body form-grid two">
        <label>
          Webchat port
          <input value="18789" />
        </label>
        <label>
          Auth token
          <input type="password" value="generated-and-hidden" autocomplete="new-password" />
        </label>
        <label>
          Allowed origin
          <input value="http://localhost:18789" />
        </label>
        <label>
          Telegram allowlist
          <input value="disabled" />
        </label>
      </div>
    </section>
    ${renderActionCards('webchat')}
  `;
}

function renderGraphSection() {
  return `
    ${renderHero(
      'Knowledge graph',
      'See how memory connects before agents use it',
      'The graph view is prepared for entity, task, source, and decision nodes from local memory.',
      [
        { label: 'Nodes', value: 'Local' },
        { label: 'Edges', value: 'Scoped' },
        { label: 'Exports', value: 'Manual' },
      ],
    )}
    <section class="panel graph-canvas">
      <div class="graph-node large" style="left: 42%; top: 36%;">Argentum</div>
      <div class="graph-node" style="left: 12%; top: 22%;">Tasks</div>
      <div class="graph-node" style="left: 16%; top: 66%;">Memory</div>
      <div class="graph-node" style="left: 70%; top: 20%;">Skills</div>
      <div class="graph-node" style="left: 72%; top: 68%;">Approvals</div>
      <div class="graph-edge edge-a"></div>
      <div class="graph-edge edge-b"></div>
      <div class="graph-edge edge-c"></div>
    </section>
    ${renderActionCards('graph')}
  `;
}

function renderMemorySection() {
  return `
    ${renderHero(
      'Memory',
      'Search and inspect local recall before using it',
      'Memory controls stay under the selected workspace, with read-only inspection separated from destructive actions.',
      [
        { label: 'Namespaces', value: '3' },
        { label: 'Default', value: 'semantic' },
        { label: 'Boundary', value: 'Workspace' },
      ],
    )}
    <section class="panel">
      <div class="panel-body form-grid">
        <label>
          Memory search
          <input value="project context" />
        </label>
      </div>
    </section>
    <div class="card-grid">
      ${['Recent decisions', 'Project facts', 'Skill notes']
        .map(
          (item) => `
            <article class="feature-card">
              <span class="pill">Namespace</span>
              <h3>${item}</h3>
              <p>Ready for search, export, or review through approved memory commands.</p>
            </article>
          `,
        )
        .join('')}
    </div>
    ${renderActionCards('memory')}
  `;
}

function renderLogsSection() {
  return `
    ${renderHero(
      'Activity logs',
      'Trace actions without exposing secrets',
      'Logs, audit entries, and command output stay readable from the app while secrets remain redacted.',
      [
        { label: 'Gateway', value: 'Ready' },
        { label: 'Audit', value: 'On' },
        { label: 'Secrets', value: 'Redacted' },
      ],
    )}
    <section class="panel log-viewer">
      <div class="panel-header split-header">
        <h3>Gateway Log</h3>
        <button class="button" data-refresh-state="true">Refresh</button>
      </div>
      <pre>${escapeHtml(state.desktopState?.gatewayLogPreview || 'No entries yet.')}</pre>
    </section>
    <section class="panel log-viewer">
      <div class="panel-header split-header">
        <h3>Audit Log</h3>
        <button class="button" data-refresh-state="true">Refresh</button>
      </div>
      <pre>${escapeHtml(state.desktopState?.auditLogPreview || 'No entries yet.')}</pre>
    </section>
    ${renderActionCards('logs')}
  `;
}

function renderSecuritySection() {
  return `
    ${renderHero(
      'Security broker',
      'Default-deny until you approve scope',
      'The GUI makes grants visible before agents can use files, shell, network, browser automation, or OS controls.',
      [
        { label: 'Profile', value: labelFor('securityProfile', state.securityProfile) },
        { label: 'Workspace', value: 'Only' },
        { label: 'Audit', value: 'Enabled' },
      ],
    )}
    <section class="panel">
      <div class="panel-header">
        <h3>Capability Queue</h3>
      </div>
      <div class="panel-body approval-list">
        ${state.pendingApprovals
          .map(
            (approval) => `
              <div class="approval-row">
                <div>
                  <strong>${escapeHtml(approval.title)}</strong>
                  <p>${escapeHtml(approval.detail)}</p>
                </div>
                <span class="pill ${approval.status === 'Blocked' ? 'warn' : 'ok'}">${escapeHtml(approval.status)}</span>
                <button class="button" data-approval="${escapeAttribute(approval.id)}">Review</button>
              </div>
            `,
          )
          .join('')}
      </div>
    </section>
    ${renderActionCards('security')}
  `;
}

function renderSettingsSection() {
  return `
    ${renderHero(
      'Settings',
      'One source of truth for app and CLI configuration',
      'Desktop controls write to the same workspace config that the CLI reads.',
      [
        { label: 'Runtime', value: labelFor('runtimeMode', state.runtimeMode) },
        { label: 'Config', value: 'YAML' },
        { label: 'Version', value: '0.0.4' },
      ],
    )}
    <section class="panel">
      <div class="panel-body form-grid two">
        <label>
          Workspace
          <input id="settings-workspace" value="${escapeAttribute(state.workspacePath)}" />
        </label>
        <label>
          Provider
          <select id="settings-provider">
            <option value="openai" ${selected(state.llmProvider, 'openai')}>OpenAI</option>
            <option value="gemini" ${selected(state.llmProvider, 'gemini')}>Google Gemini</option>
            <option value="anthropic" ${selected(state.llmProvider, 'anthropic')}>Anthropic</option>
            <option value="local" ${selected(state.llmProvider, 'local')}>Local model endpoint</option>
          </select>
        </label>
        <label>
          Permission profile
          <select id="settings-security">
            <option value="restricted" ${selected(state.securityProfile, 'restricted')}>Restricted: workspace-only</option>
            <option value="ask" ${selected(state.securityProfile, 'ask')}>Ask every time</option>
            <option value="session" ${selected(state.securityProfile, 'session')}>Session grants</option>
            <option value="trusted" ${selected(state.securityProfile, 'trusted')}>Trusted mode</option>
          </select>
        </label>
        <label>
          Channel mode
          <select id="settings-channel">
            <option value="local-only" ${selected(state.channelMode, 'local-only')}>Local app only</option>
            <option value="webchat" ${selected(state.channelMode, 'webchat')}>Local webchat</option>
            <option value="telegram" ${selected(state.channelMode, 'telegram')}>Telegram with allowlist</option>
          </select>
        </label>
      </div>
    </section>
    ${renderActionCards('settings')}
  `;
}

function renderDiagnosticsSection() {
  return `
    ${renderHero(
      'Diagnostics',
      'Run the same health checks from the desktop',
      'Doctor checks stay available in both the GUI and terminal so support paths are consistent.',
      [
        { label: 'Config', value: state.setupComplete ? 'Saved' : 'Pending' },
        { label: 'Build', value: 'Desktop' },
        { label: 'Security', value: 'On' },
      ],
    )}
    <section class="panel">
      <div class="panel-body check-grid">
        ${[
          ['Node runtime', 'Installed by release package'],
          [
            'Config',
            state.desktopState?.configExists
              ? state.desktopState.configPath || state.savedConfigPath
              : 'Missing until setup is saved',
          ],
          ['Data directory', formatFound(state.desktopState?.dataExists)],
          ['Logs directory', formatFound(state.desktopState?.logsExists)],
          [
            'Gateway',
            state.desktopState?.gatewayPid ? `PID ${state.desktopState.gatewayPid}` : 'Stopped',
          ],
          ['Workspace boundary', `${state.workspacePath} (${formatWorkspaceHealth()})`],
          ['Tauri bridge', window.__TAURI__ ? 'Connected' : 'Preview mode'],
          ['Capability broker', labelFor('securityProfile', state.securityProfile)],
          ['Secrets', 'Stored outside YAML'],
        ]
          .map(
            ([name, detail]) => `
              <div class="check-row">
                <span class="pill ok">OK</span>
                <strong>${escapeHtml(name)}</strong>
                <p>${escapeHtml(detail)}</p>
              </div>
            `,
          )
          .join('')}
      </div>
    </section>
    ${renderActionCards('diagnostics')}
  `;
}

const sectionRenderers = {
  chat: renderChatSection,
  agents: renderAgentsSection,
  runner: renderRunnerSection,
  skills: renderSkillsSection,
  webchat: renderWebchatSection,
  graph: renderGraphSection,
  memory: renderMemorySection,
  logs: renderLogsSection,
  security: renderSecuritySection,
  settings: renderSettingsSection,
  diagnostics: renderDiagnosticsSection,
};

function renderSection() {
  const section = sections.find((item) => item.id === state.activeSection) || sections[0];
  title.textContent = section.title;
  eyebrow.textContent = section.eyebrow;

  const renderer = sectionRenderers[section.id] || renderChatSection;
  viewRoot.innerHTML = renderer();

  bindSelect('#settings-provider', (value) => {
    state.llmProvider = value;
  });
  bindSelect('#settings-security', (value) => {
    state.securityProfile = value;
  });
  bindSelect('#settings-channel', (value) => {
    state.channelMode = value;
  });

  const settingsWorkspace = document.querySelector('#settings-workspace');
  if (settingsWorkspace) {
    settingsWorkspace.addEventListener('input', (event) => {
      state.workspacePath = event.target.value;
      workspacePath.textContent = state.workspacePath;
    });
  }
}

function render() {
  workspacePath.textContent = state.workspacePath;
  renderNavigation();
  if (state.activeSection === 'onboarding') renderOnboarding();
  else renderSection();
}

nav.addEventListener('click', (event) => {
  const button = event.target.closest('[data-section]');
  if (!button) return;

  state.activeSection = button.dataset.section;
  render();
});

viewRoot.addEventListener('click', (event) => {
  const interfaceButton = event.target.closest('[data-interface-mode]');
  if (interfaceButton) {
    state.runtimeMode = interfaceButton.dataset.interfaceMode;
    render();
    return;
  }

  const stepButton = event.target.closest('[data-onboarding-step]');
  if (stepButton) {
    state.onboardingStep = Number(stepButton.dataset.onboardingStep);
    render();
    return;
  }

  const copyButton = event.target.closest('[data-copy-command]');
  if (copyButton) {
    copyCommand(copyButton.dataset.copyCommand);
    return;
  }

  const actionButton = event.target.closest('[data-run-action]');
  if (actionButton) {
    runAction(actionButton.dataset.runAction);
    return;
  }

  const refreshButton = event.target.closest('[data-refresh-state]');
  if (refreshButton) {
    refreshDesktopState({ announce: true });
    return;
  }

  const approvalButton = event.target.closest('[data-approval]');
  if (approvalButton) {
    const approval = state.pendingApprovals.find(
      (item) => item.id === approvalButton.dataset.approval,
    );
    if (approval) {
      state.actionStatus = `Review opened for: ${approval.title}`;
      render();
    }
  }
});

viewRoot.addEventListener('input', (event) => {
  if (event.target.id === 'chat-draft') {
    state.draftMessage = event.target.value;
  }
});

viewRoot.addEventListener('click', (event) => {
  if (event.target.id !== 'send-chat') return;
  const draft = state.draftMessage.trim();
  if (!draft) return;

  state.chatMessages.push({ speaker: 'You', text: draft });
  state.chatMessages.push({
    speaker: 'Argentum',
    text: 'Message staged in the desktop session. Start the gateway to connect live model execution.',
  });
  state.draftMessage = '';
  state.actionStatus = 'Chat message staged.';
  render();
});

document.querySelector('#audit-button').addEventListener('click', () => {
  state.activeSection = 'security';
  render();
});

document.querySelector('#settings-button').addEventListener('click', () => {
  state.activeSection = 'settings';
  render();
});

render();
hydrateDesktopDefaults().then(() => refreshDesktopState());
