import { providerAuthMethods, providerPresets, runtimeModes, securityProfiles } from './constants.js';
import { chatModule } from './chat.js';
import { onboardingModule } from './onboarding.js';
import {
  formatFound,
  formatWorkspaceHealth,
  renderActionCards,
  renderHero,
  renderNotifications,
} from './shell.js';
import { currentProvider, escapeAttribute, escapeHtml, labelFor, modelOptionsFor, selected } from './utils.js';

function terminalPreview(state, filter = '') {
  const entries = filter
    ? state.terminalEntries.filter((entry) => entry.command.includes(filter))
    : state.terminalEntries;

  return entries.length === 0
    ? 'No action output yet.'
    : entries
        .slice(0, 5)
        .map((entry) => `$ ${entry.command}\n${entry.output}`)
        .join('\n\n');
}

function gatewayModule() {
  return {
    id: 'gateway',
    label: 'Gateway',
    validate: () => '',
    healthCheck: (state) => ({
      status: state.desktopState?.gatewayPid ? 'ok' : 'stopped',
      message: state.desktopState?.gatewayPid
        ? `Gateway is running as PID ${state.desktopState.gatewayPid}.`
        : 'Gateway is stopped.',
    }),
    render: (state) => {
      const running = Boolean(state.desktopState?.gatewayPid);
      const healthUrl = running ? 'http://127.0.0.1:3000/health' : 'Available after start';

      return `
        ${renderNotifications()}
        ${renderHero(
          'Gateway',
          running ? 'Local gateway is running' : 'Local gateway is stopped',
          'Start, stop, inspect, and read logs for the workspace gateway. These buttons execute fixed Argentum commands through the native bridge.',
          [
            { label: 'State', value: running ? 'Running' : 'Stopped' },
            { label: 'PID', value: state.desktopState?.gatewayPid || 'None' },
            { label: 'Health', value: healthUrl },
          ],
        )}
        <div class="gateway-grid">
          <section class="panel">
            <div class="panel-header">
              <h3>Controls</h3>
              <p>No arbitrary shell is exposed here. Each button maps to a whitelisted gateway action.</p>
            </div>
            ${renderActionCards('gateway')}
          </section>
          <section class="panel terminal-panel">
            <div class="panel-header split-header">
              <h3>Gateway Output</h3>
              <button class="button" data-run-action="gateway-status">Refresh</button>
            </div>
            <div class="terminal-body">
              <article class="terminal-entry info">
                <pre>${escapeHtml(terminalPreview(state, 'gateway'))}</pre>
              </article>
            </div>
          </section>
        </div>
      `;
    },
  };
}

const logsModule = {
  id: 'logs',
  label: 'Activity Logs',
  validate: () => '',
  healthCheck: (state) => ({
    status: state.desktopState?.logsExists ? 'ok' : 'pending',
    message: state.desktopState?.logsExists ? 'Logs directory exists.' : 'Logs appear after setup or runtime activity.',
  }),
  render: (state) => `
    ${renderNotifications()}
    ${renderHero(
      'Activity logs',
      'Trace actions without exposing secrets',
      'Logs, audit entries, and command output stay readable from the app while sensitive lines are redacted.',
      [
        { label: 'Gateway', value: state.desktopState?.gatewayPid ? `PID ${state.desktopState.gatewayPid}` : 'Stopped' },
        { label: 'Audit', value: 'On' },
        { label: 'Secrets', value: 'Redacted' },
      ],
    )}
    <section class="panel log-viewer">
      <div class="panel-header split-header">
        <h3>Gateway Log</h3>
        <div class="button-row">
          <button class="button" data-run-action="gateway-logs">Load gateway logs</button>
          <button class="button" data-refresh-state="true">Refresh</button>
        </div>
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
  `,
};

const securityModule = {
  id: 'security',
  label: 'Security & Permissions',
  validate: () => '',
  healthCheck: () => ({ status: 'ok', message: 'Permission broker is visible.' }),
  render: (state) => `
    ${renderNotifications()}
    ${renderHero(
      'Security broker',
      'Default-deny until you approve scope',
      'The GUI makes grants visible before agents can use files, shell, network, browser automation, OS controls, or repair actions.',
      [
        { label: 'Profile', value: labelFor(securityProfiles, state.securityProfile) },
        { label: 'Workspace', value: 'Only by default' },
        { label: 'Audit', value: 'Enabled' },
      ],
    )}
    <section class="panel">
      <div class="panel-header">
        <h3>Current Boundary</h3>
        <p>Default access means all folders and files inside the workspace folder below. Anything outside it must be explicitly approved.</p>
      </div>
      <div class="panel-body status-stack">
        <div>
          <span>Workspace folder</span>
          <strong>${escapeHtml(state.workspacePath)}</strong>
        </div>
        <div>
          <span>Permission profile</span>
          <strong>${escapeHtml(labelFor(securityProfiles, state.securityProfile))}</strong>
        </div>
        <div>
          <span>Privileged actions</span>
          <strong>Ask first</strong>
        </div>
      </div>
    </section>
    <section class="panel">
      <div class="panel-header">
        <h3>Capability Queue</h3>
        <p>These rows are visible policy states. Execution controls appear only when the broker has a concrete request to approve.</p>
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
              </div>
            `,
          )
          .join('')}
      </div>
    </section>
  `,
};

const settingsModule = {
  id: 'settings',
  label: 'Settings',
  validate: () => '',
  healthCheck: () => ({ status: 'ok', message: 'Settings are editable.' }),
  render: (state) => {
    const provider = currentProvider(providerPresets, state);
    return `
      ${renderNotifications()}
      ${renderHero(
        'Settings',
        'One source of truth for app and CLI configuration',
        'Desktop controls save to the same workspace config that the CLI reads. Restart onboarding for full setup changes, or test the current provider here.',
        [
          { label: 'Runtime', value: labelFor(runtimeModes, state.runtimeMode) },
          { label: 'Provider', value: provider.label },
          { label: 'Config', value: state.desktopState?.configExists ? 'Saved' : 'Pending' },
        ],
      )}
      <section class="panel">
        <div class="panel-body form-grid two">
          <label>
            Workspace
            <input id="settings-workspace" value="${escapeHtml(state.workspacePath)}" />
          </label>
          <label>
            Provider
            <select id="settings-provider">
              ${providerPresets
                .map(
                  (item) => `
                    <option value="${item.id}" ${item.id === state.llmProvider ? 'selected' : ''}>${escapeHtml(item.label)}</option>
                  `,
                )
                .join('')}
            </select>
          </label>
          <label>
            Model
            <select id="settings-provider-model">
              ${modelOptionsFor(provider, state.providerModel)
                .map(
                  (model) => `
                    <option value="${escapeAttribute(model.id)}" ${selected(state.providerModel, model.id)}>${escapeHtml(model.label || model.id)}</option>
                  `,
                )
                .join('')}
            </select>
          </label>
          <label>
            Endpoint
            <input id="settings-provider-base-url" value="${escapeAttribute(state.providerBaseUrl)}" />
          </label>
          <label>
            Authorization method
            <select id="settings-provider-auth-method">
              ${providerAuthMethods
                .map(
                  (method) => `
                    <option value="${escapeAttribute(method.id)}" ${selected(state.providerAuthMethod, method.id)} ${method.disabled ? 'disabled' : ''}>${escapeHtml(method.label)} - ${escapeHtml(method.status)}</option>
                  `,
                )
                .join('')}
            </select>
          </label>
          <label>
            Permission profile
            <select id="settings-security">
              ${securityProfiles
                .map(
                  (item) => `
                    <option value="${item.id}" ${item.id === state.securityProfile ? 'selected' : ''}>${escapeHtml(item.label)}</option>
                  `,
                )
                .join('')}
            </select>
          </label>
          <label>
            Runtime
            <select id="settings-runtime">
              ${runtimeModes
                .map(
                  (item) => `
                    <option value="${item.id}" ${item.id === state.runtimeMode ? 'selected' : ''}>${escapeHtml(item.label)}</option>
                  `,
                )
                .join('')}
            </select>
          </label>
        </div>
        ${renderSettingsOAuthPanel(state)}
        <div class="panel-footer button-row split">
          <button class="button" id="test-provider">Test Provider</button>
          <button class="button" data-restart-onboarding="true">Restart onboarding</button>
        </div>
      </section>
    `;
  },
};

function renderSettingsOAuthPanel(state) {
  const oauth = state.codexOAuth || {};
  const verificationUrl = oauth.verificationUrl || 'https://auth.openai.com/codex/device';

  return `
    <div class="panel-body oauth-panel ${state.providerAuthMethod === 'browser-account' ? 'active' : ''}">
      <div>
        <span class="pill">OpenAI/Codex OAuth</span>
        <h3>Browser Account Authorization</h3>
        <p>Use this only for OpenAI. Start authorization, approve it in your browser, then complete it here. API key auth remains available above.</p>
      </div>
      <div class="oauth-actions">
        <button class="button" id="start-codex-oauth">Start OpenAI/Codex authorization</button>
        <button class="button primary" id="complete-codex-oauth">Complete authorization</button>
      </div>
      <div class="oauth-code-grid">
        <div>
          <span>Verification page</span>
          <a href="${escapeAttribute(verificationUrl)}" target="_blank" rel="noreferrer">${escapeHtml(verificationUrl)}</a>
        </div>
        <div>
          <span>User code</span>
          <strong>${escapeHtml(oauth.userCode || 'Not requested')}</strong>
        </div>
        <div>
          <span>Credential folder</span>
          <strong>${escapeHtml(oauth.codexHome || 'Inside workspace/data after authorization')}</strong>
        </div>
      </div>
      <p>${escapeHtml(oauth.message || 'OpenAI/Codex authorization has not been started.')}</p>
    </div>
  `;
}

const diagnosticsModule = {
  id: 'diagnostics',
  label: 'Diagnostics',
  validate: () => '',
  healthCheck: (state) => ({
    status: state.desktopState?.configExists ? 'ok' : 'pending',
    message: state.desktopState?.configExists ? 'Configuration exists.' : 'Configuration has not been saved yet.',
  }),
  render: (state) => {
    const checks = [
      ['Config', state.desktopState?.configExists ? state.desktopState.configPath || state.savedConfigPath : 'Missing until setup is saved', state.desktopState?.configExists],
      ['Data directory', formatFound(state.desktopState?.dataExists), state.desktopState?.dataExists],
      ['Logs directory', formatFound(state.desktopState?.logsExists), state.desktopState?.logsExists],
      ['Gateway', state.desktopState?.gatewayPid ? `PID ${state.desktopState.gatewayPid}` : 'Stopped', true],
      ['Workspace boundary', `${state.workspacePath} (${formatWorkspaceHealth()})`, Boolean(state.workspacePath)],
      ['Tauri bridge', window.__TAURI__ ? 'Connected' : 'Preview mode', true],
      ['Provider', state.apiTest.message, state.apiTest.status !== 'error'],
      ['Capability broker', labelFor(securityProfiles, state.securityProfile), true],
      ['Secrets', 'Stored outside YAML', true],
    ];

    return `
      ${renderNotifications()}
      ${renderHero(
        'Diagnostics',
        'Health checks and repair suggestions',
        'Checks are modular. A failed provider or gateway check does not stop Chat or Security from opening.',
        [
          { label: 'Config', value: state.setupComplete ? 'Saved' : 'Pending' },
          { label: 'Gateway', value: state.desktopState?.gatewayPid ? 'Running' : 'Stopped' },
          { label: 'Modules', value: 'Contained' },
        ],
      )}
      <section class="panel">
        <div class="panel-header split-header">
          <h3>Health Checks</h3>
          <button class="button" data-refresh-state="true">Refresh</button>
        </div>
        <div class="panel-body check-grid">
          ${checks
            .map(
              ([name, detail, ok]) => `
                <div class="check-row">
                  <span class="pill ${ok ? 'ok' : 'warn'}">${ok ? 'OK' : 'Check'}</span>
                  <strong>${escapeHtml(name)}</strong>
                  <p>${escapeHtml(detail)}</p>
                </div>
              `,
            )
            .join('')}
        </div>
      </section>
    `;
  },
};

export const modules = {
  onboarding: onboardingModule,
  chat: chatModule,
  gateway: gatewayModule(),
  logs: logsModule,
  security: securityModule,
  settings: settingsModule,
  diagnostics: diagnosticsModule,
};
