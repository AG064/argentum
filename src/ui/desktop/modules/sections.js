import {
  ARGENTUM_BUNDLED_SKILLS,
  contextAccessOptions,
  fontOptions,
  llamaDownloadPresets,
  modelMetadata,
  providerAuthMethods,
  providerCatalogTabs,
  providerPresets,
  runtimeModes,
  securityProfiles,
  thinkingLevels,
} from './constants.js';
import { SUPPORTED_LOCALES } from '../i18n/index.js';

import { chatModule } from './chat.js';
import { onboardingModule } from './onboarding.js';
import { terminalEntriesForDisplay } from './state.js';
import {
  formatFound,
  formatWorkspaceHealth,
  renderActionCards,
  renderNotifications,
} from './shell.js';
import {
  checked,
  currentProvider,
  displayModelName,
  escapeAttribute,
  escapeHtml,
  estimateRuntimeContextTokens,
  labelFor,
  modelMetadataFor,
  modelOptionsFor,
  selected,
} from './utils.js';
import {
  detectMigrationSources,
  handleMigrationImport,
  openExternalUrl,
} from './setup.js';

function terminalPreview(state, filter = '') {
  const entries = terminalEntriesForDisplay(filter);

  const actionOutput =
    entries.length === 0
      ? ''
      : entries.map((entry) => `$ ${entry.command}\n${entry.output}`).join('\n\n');
  const logOutput = filter === 'gateway' ? state.desktopState?.gatewayLogPreview || '' : '';

  return (
    [actionOutput, logOutput]
      .filter((part) => part && part !== 'No entries yet.')
      .join('\n\n')
      .trim() || 'No action output yet.'
  );
}

function renderActivityEventLog(state) {
  const events = state.appLogEntries || [];
  return `
    <section class="panel log-viewer app-event-log glass-panel">
      <div class="panel-header split-header">
        <h3>Activity event log</h3>
        <span class="pill">${events.length} events</span>
      </div>
      ${
        events.length === 0
          ? `<pre>${escapeHtml(state.desktopState?.appLogPreview || 'No structured app events yet. Provider tests, chat sends, gateway actions, settings saves, and diagnostics will appear here.')}</pre>`
          : `<div class="activity-event-list">
              ${events
                .slice(0, 12)
                .map(
                  (event) => `
                    <article>
                      <span class="pill ${event.status === 'error' ? 'warn' : 'ok'}">${escapeHtml(event.status)}</span>
                      <strong>${escapeHtml(event.event)}</strong>
                      <p>${escapeHtml(event.message)}</p>
                      <time>${escapeHtml(new Date(event.timestamp).toLocaleString())}</time>
                    </article>
                  `,
                )
                .join('')}
            </div>`
      }
    </section>
  `;
}

function renderUsageWindows(usage) {
  const windows = Array.isArray(usage?.modalityQuotas) ? usage.modalityQuotas : [];
  const fallback = [
    {
      label: 'Provider requests',
      remaining: usage?.requestRemaining || 'Unknown',
      limit: usage?.fiveHourRequestLimit || usage?.requestLimit || 'Not reported',
      resetCadence: usage?.requestResetCadence || usage?.resetCadence || 'Reset not reported',
    },
    {
      label: 'Provider tokens',
      remaining: usage?.tokenRemaining || 'Unknown',
      limit: usage?.tokenLimit || 'Not reported',
      resetCadence: usage?.tokenResetCadence || usage?.resetCadence || 'Reset not reported',
    },
  ];
  const source = windows.length ? windows : fallback;

  return `
    <div class="usage-window-grid">
      ${source
        .map(
          (window) => `
            <div>
              <span>${escapeHtml(window.label)}</span>
              <strong>${escapeHtml(window.remaining || 'Unknown')}</strong>
              <small>${escapeHtml(window.limit ? `Limit: ${window.limit}` : 'Limit not reported')}</small>
              <small>${escapeHtml(window.resetCadence || window.reset || 'Reset not reported')}</small>
            </div>
          `,
        )
        .join('')}
      ${
        usage?.accountUsageStatus
          ? `<div>
              <span>Account page</span>
              <strong>${escapeHtml(usage.accountUsageStatus)}</strong>
              <small>${escapeHtml(usage.accountUsageSource || 'Browser-profile usage source unavailable')}</small>
              ${
                usage.accountUsageUrl
                  ? `<a class="provider-website-link" href="${escapeAttribute(usage.accountUsageUrl)}" data-open-external="${escapeAttribute(usage.accountUsageUrl)}">Open usage page <span data-icon="externalLink"></span></a>`
                  : ''
              }
            </div>`
          : ''
      }
    </div>
  `;
}

function renderCompactPageHeader({
  kicker,
  title,
  detail,
  stats = [],
  actions = '',
  tone = '',
  statsClass = '',
}) {
  return `
    <section class="compact-page-header ${escapeAttribute(tone)}">
      <div class="compact-page-title">
        <span class="eyebrow">${escapeHtml(kicker)}</span>
        <h2>${escapeHtml(title)}</h2>
        <p>${escapeHtml(detail)}</p>
      </div>
      ${
        stats.length
          ? `<div class="compact-status-strip ${escapeAttribute(statsClass)}">
              ${stats
                .map(
                  (stat) => `
                    <div class="compact-stat">
                      <span>${escapeHtml(stat.label)}</span>
                      <strong>${escapeHtml(stat.value)}</strong>
                      ${stat.detail ? `<small>${escapeHtml(stat.detail)}</small>` : ''}
                    </div>
                  `,
                )
                .join('')}
            </div>`
          : ''
      }
      ${actions ? `<div class="compact-toolbar">${actions}</div>` : ''}
    </section>
  `;
}

function renderProductTabShell(id, body) {
  return `
    <div class="product-tab-shell ${escapeAttribute(id)}-tab">
      ${body}
    </div>
  `;
}

function formatBytes(value) {
  const bytes = Number(value || 0);
  if (!Number.isFinite(bytes) || bytes <= 0) return 'Unavailable';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  return `${size >= 10 || unitIndex === 0 ? size.toFixed(0) : size.toFixed(1)} ${units[unitIndex]}`;
}

function formatPercent(value) {
  const percentValue = Number(value || 0);
  if (!Number.isFinite(percentValue)) return '0%';
  return `${Math.max(0, Math.min(100, percentValue)).toFixed(0)}%`;
}

function formatDuration(seconds) {
  const totalSeconds = Number(seconds || 0);
  if (!Number.isFinite(totalSeconds) || totalSeconds <= 0) return 'Unavailable';
  const days = Math.floor(totalSeconds / 86400);
  const hours = Math.floor((totalSeconds % 86400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

function renderStatBar(value, label) {
  const percentValue = Math.max(0, Math.min(100, Number(value || 0)));
  return `
    <div class="pc-stat-bar" aria-label="${escapeAttribute(label)} ${formatPercent(percentValue)}">
      <span style="width: ${percentValue}%"></span>
    </div>
  `;
}

function renderCommandDock(actions) {
  return actions
    .map(
      (action) => `
        <button class="button ${action.primary ? 'primary' : ''}" ${action.actionId ? `data-run-action="${escapeAttribute(action.actionId)}"` : ''} ${action.refresh ? 'data-refresh-state="true"' : ''}>
          ${escapeHtml(action.label)}
        </button>
      `,
    )
    .join('');
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
        ${renderProductTabShell(
          'gateway',
          `
            ${renderCompactPageHeader({
              kicker: 'Runtime gateway',
              title: running ? 'Local gateway is running' : 'Local gateway is stopped',
              detail:
                "Argentum's gateway is the local service entrance. It only exposes approved localhost APIs and integrations after you start it from a fixed desktop command.",
              tone: running ? 'online' : 'standby',
              stats: [
                { label: 'State', value: running ? 'Running' : 'Stopped' },
                { label: 'PID', value: state.desktopState?.gatewayPid || 'None' },
                { label: 'Health', value: healthUrl },
              ],
              statsClass: 'gateway-status-grid',
              actions: renderCommandDock([
                { label: 'Start Gateway', actionId: 'gateway-start', primary: true },
                { label: 'Check Status', actionId: 'gateway-status' },
                { label: 'Stop Gateway', actionId: 'gateway-stop' },
                { label: 'View Logs', actionId: 'gateway-logs' },
              ]),
            })}
            <div class="product-content-grid gateway-grid terminal-first">
              <details class="panel terminal-panel gateway-terminal-shell compact-detail" open>
                <summary>
                  <span>
                    <em>Terminal</em>
                    <strong>Gateway Output</strong>
                  </span>
                  <small>Collapse</small>
                </summary>
                <div class="terminal-toolbar">
                  <button class="button compact" type="button" data-run-action="gateway-status">Refresh</button>
                </div>
                <div class="terminal-body">
                  <article class="terminal-entry info">
                    <pre>${escapeHtml(terminalPreview(state, 'gateway'))}</pre>
                  </article>
                </div>
              </details>
              <details class="panel gateway-actions-panel compact-detail">
                <summary>
                  <span>
                    <em>Options</em>
                    <strong>Gateway commands</strong>
                  </span>
                  <small>Expand</small>
                </summary>
                <div class="panel-header">
                  <span class="pill">Whitelisted</span>
                  <h3>Commands</h3>
                </div>
                ${renderActionCards('gateway')}
              </details>
            </div>
          `,
        )}
      `;
    },
  };
}

function localServerModule() {
  return {
    id: 'local-server',
    label: 'Local Server',
    validate: () => '',
    healthCheck: (state) => ({
      status: state.desktopState?.llamaServerPid
        ? 'ok'
        : state.desktopState?.llamaServerInstalled
          ? 'stopped'
          : 'missing',
      message: state.desktopState?.llamaServerPid
        ? `Argentum llama.cpp is running as PID ${state.desktopState.llamaServerPid}.`
        : state.desktopState?.llamaServerInstalled
          ? 'Argentum llama.cpp is installed but stopped.'
          : 'Argentum llama.cpp binary is not installed in this build.',
    }),
    render: (state) => {
      const running = Boolean(state.desktopState?.llamaServerPid);
      const installed = Boolean(state.desktopState?.llamaServerInstalled);
      const endpoint = state.desktopState?.llamaServerEndpoint || 'http://127.0.0.1:8080/v1';
      const progress = state.llamaServerProgress;

      return `
        ${renderNotifications()}
        ${renderProductTabShell(
          'local-server',
          `
              ${renderCompactPageHeader({
                kicker: 'Local model runtime',
                title: running
                  ? 'Argentum llama.cpp is running'
                  : installed
                    ? 'Argentum llama.cpp is installed'
                    : 'Argentum llama.cpp is not installed',
                detail:
                  'Argentum can run a localhost llama.cpp server from a selected GGUF file or a vetted Hugging Face download preset. The GUI translates safe settings into fixed llama-server flags.',
                tone: running ? 'online' : installed ? 'standby' : 'warning',
                stats: [
                  {
                    label: 'State',
                    value: running ? 'Running' : installed ? 'Stopped' : 'Missing',
                  },
                  { label: 'PID', value: state.desktopState?.llamaServerPid || 'None' },
                  { label: 'Endpoint', value: endpoint },
                ],
                statsClass: 'gateway-status-grid',
                actions: renderCommandDock([
                  ...(installed
                    ? []
                    : [
                        {
                          label: 'Install Server',
                          actionId: 'llama-server-install',
                          primary: true,
                        },
                      ]),
                  { label: 'Start Server', actionId: 'llama-server-start', primary: true },
                  { label: 'Check Status', actionId: 'llama-server-status' },
                  { label: 'Stop Server', actionId: 'llama-server-stop' },
                  { label: 'View Logs', actionId: 'llama-server-logs' },
                ]),
              })}
              ${
                progress
                  ? `
                    <section class="panel compact-panel llama-progress-panel">
                      <div class="split-header">
                        <div>
                          <span class="eyebrow">Model setup</span>
                          <h3>${escapeHtml(progress.phase || 'Preparing llama.cpp')}</h3>
                        </div>
                        <strong>${Math.max(0, Math.min(100, Math.round(progress.percent || 0)))}%</strong>
                      </div>
                      <div class="llama-progress-track" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${Math.max(0, Math.min(100, Math.round(progress.percent || 0)))}">
                        <span style="width:${Math.max(0, Math.min(100, Number(progress.percent || 0)))}%"></span>
                      </div>
                      <p>${escapeHtml(progress.detail || 'Waiting for llama.cpp output.')}</p>
                    </section>
                  `
                  : ''
              }
              <div class="product-content-grid gateway-grid terminal-first">
              <details class="panel terminal-panel gateway-terminal-shell compact-detail" open>
                <summary>
                  <span>
                    <em>Terminal</em>
                    <strong>llama.cpp Output</strong>
                  </span>
                  <small>Collapse</small>
                </summary>
                <div class="terminal-toolbar">
                  <button class="button compact" type="button" data-run-action="llama-server-status">Refresh</button>
                </div>
                <div class="terminal-body">
                  <article class="terminal-entry info">
                    <pre>${escapeHtml(terminalPreview(state, 'llama-server') || state.desktopState?.llamaServerLogPreview || 'No entries yet.')}</pre>
                  </article>
                </div>
              </details>
              <details class="panel gateway-actions-panel compact-detail">
                <summary>
                  <span>
                    <em>Options</em>
                    <strong>Server controls</strong>
                  </span>
                  <small>Expand</small>
                </summary>
                <div class="panel-header">
                  <span class="pill">Stable local</span>
                  <h3>Server controls</h3>
                </div>
                ${renderActionCards('local-server')}
                <div class="settings-inline-note">
                  <strong>Optional binary</strong>
                  <p>The Windows setup can install llama.cpp only when its optional checkbox is selected. If skipped, use Install Server here, set LLAMA_SERVER_BIN, or place llama-server in workspace/bin.</p>
                </div>
              </details>
            </div>
          `,
        )}
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
    message: state.desktopState?.logsExists
      ? 'Logs directory exists.'
      : 'Logs appear after setup or runtime activity.',
  }),
  render: (state) => `
    ${renderNotifications()}
    ${renderProductTabShell(
      'logs',
      `
        ${renderCompactPageHeader({
          kicker: 'Activity logs',
          title: 'Trace actions without exposing secrets',
          detail:
            'Logs, audit entries, and command output stay readable from the app while sensitive lines are redacted and high-signal events remain searchable.',
          tone: 'logs',
          stats: [
            {
              label: 'Gateway',
              value: state.desktopState?.gatewayPid
                ? `PID ${state.desktopState.gatewayPid}`
                : 'Stopped',
            },
            { label: 'Audit', value: 'On' },
            { label: 'Secrets', value: 'Redacted' },
          ],
          actions: renderCommandDock([
            { label: 'Load gateway logs', actionId: 'gateway-logs', primary: true },
            { label: 'Refresh', refresh: true },
          ]),
        })}
        <div class="product-content-grid log-console-grid">
          <section class="panel log-viewer glass-panel">
            <div class="panel-header split-header">
              <div>
                <span class="pill">Runtime</span>
                <h3>Gateway Log</h3>
              </div>
              <button class="button" data-run-action="gateway-logs">Load gateway logs</button>
            </div>
            <pre>${escapeHtml(state.desktopState?.gatewayLogPreview || 'No entries yet.')}</pre>
          </section>
          <section class="panel log-viewer glass-panel">
            <div class="panel-header split-header">
              <div>
                <span class="pill">Audit</span>
                <h3>Capability Log</h3>
              </div>
              <button class="button" data-refresh-state="true">Refresh</button>
            </div>
            <pre>${escapeHtml(state.desktopState?.auditLogPreview || 'No entries yet.')}</pre>
          </section>
        </div>
        ${renderActivityEventLog(state)}
      `,
    )}
  `,
};

const securityModule = {
  id: 'security',
  label: 'Security & Permissions',
  validate: () => '',
  healthCheck: () => ({ status: 'ok', message: 'Permission broker is visible.' }),
  render: (state) => `
    ${renderNotifications()}
    ${renderProductTabShell(
      'security',
      `
        ${renderCompactPageHeader({
          kicker: 'Security broker',
          title: 'Default-deny until you approve scope',
          detail:
            'The GUI makes grants visible before agents can use files, shell, network, browser automation, OS controls, or repair actions.',
          tone: 'security',
          stats: [
            { label: 'Profile', value: labelFor(securityProfiles, state.securityProfile) },
            { label: 'Workspace', value: 'Only by default' },
            { label: 'Audit', value: 'Enabled' },
          ],
        })}
        <div class="security-map-grid">
          <section class="panel glass-panel security-control-panel">
            <div class="panel-header">
              <span class="pill">Policy</span>
              <h3>Security Settings</h3>
              <p>Change the permission profile and decide which app facts Argentum can include in model context. These toggles do not grant arbitrary local access.</p>
            </div>
            <div class="panel-body security-settings-grid">
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
              ${renderContextAccessCards(state)}
            </div>
          </section>
          <section class="panel glass-panel boundary-card">
            <div class="panel-header">
              <span class="pill">Boundary</span>
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
          <section class="panel glass-panel approval-console">
            <div class="panel-header">
              <span class="pill">Queue</span>
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
        </div>
      `,
    )}
  `,
};

const pcStatsModule = {
  id: 'pc-stats',
  label: 'Argentum System Dashboard',
  validate: () => '',
  healthCheck: (state) => ({
    status: state.desktopState?.systemStats ? 'ok' : 'pending',
    message: state.desktopState?.systemStats
      ? 'Argentum System Dashboard is receiving desktop bridge statistics.'
      : 'Argentum System Dashboard needs the installed desktop app for live data.',
  }),
  render: (state) => {
    const dashboardEnabled = state.selectedContextAccess.includes('system-dashboard');

    if (!dashboardEnabled) {
      return `
        ${renderNotifications()}
        ${renderProductTabShell(
          'pc-stats',
          `
            <section class="panel glass-panel dashboard-permission-panel">
              <div class="panel-header split-header">
                <div>
                  <span class="pill">Permission required</span>
                  <h3>Enable live system dashboard</h3>
                  <p>Argentum collects local telemetry only while this tab is open. Slow sensor helpers run hidden and only when the OS needs them.</p>
                </div>
              </div>
              <label class="setting-row toggle-row">
                <span>
                  <strong>Allow local system telemetry</strong>
                  <small>CPU, memory, processes, disks, network, temperatures, and GPU inventory for this dashboard.</small>
                </span>
                <input type="checkbox" data-context-access="system-dashboard" />
              </label>
            </section>
          `,
        )}
      `;
    }

    return `
      ${renderNotifications()}
      ${renderProductTabShell(
        'pc-stats',
        `
          <section class="panel glass-panel system-dashboard-frame-panel">
            <div class="system-dashboard-frame-shell">
              <iframe
                class="system-dashboard-frame"
                title="Argentum System Dashboard"
                src="./dashboard/index.html"
                data-system-dashboard-frame="true"
                loading="lazy"
                sandbox="allow-scripts allow-same-origin"
              ></iframe>
            </div>
          </section>
        `,
      )}
    `;
  },
};

const settingsModule = {
  id: 'settings',
  label: 'Settings',
  validate: () => '',
  healthCheck: () => ({ status: 'ok', message: 'Settings are editable.' }),
  render: (state) => {
    const provider = currentProvider(providerPresets, state);
    const metadata = modelMetadataFor(state.providerModel, modelMetadata);
    const activeSection = state.settingsSection || 'overview';
    return `
      ${renderNotifications()}
      ${renderProductTabShell(
        'settings',
        `
          <section class="panel settings-workbench polished-settings glass-panel">
            <div class="settings-page-header compact">
              <div>
                <span class="eyebrow">Preferences</span>
                <h2>Settings</h2>
                <p>Open one section at a time. Keys stay in workspace secrets.</p>
              </div>
              <div class="settings-status-strip">
                <div><span>Provider</span><strong>${escapeHtml(provider.label)}</strong></div>
                <div><span>Model</span><strong>${escapeHtml(displayModelName(state.providerModel))}</strong></div>
                <div><span>Config</span><strong>${state.desktopState?.configExists ? 'Saved' : 'Pending'}</strong></div>
              </div>
            </div>
            <div class="settings-layout sectioned">
              <nav class="settings-section-nav" aria-label="Settings sections">
                ${renderSettingsNav(activeSection)}
              </nav>
              <div class="settings-fields">
                ${renderSettingsContent(state, activeSection, provider, metadata)}
              </div>
            </div>
            <div class="panel-footer button-row split">
              <button class="button" id="save-settings">Save Settings</button>
              <button class="button" id="test-provider">Test Provider</button>
              <button class="button" data-restart-onboarding="true">Restart onboarding</button>
            </div>
          </section>
        `,
      )}
    `;
  },
};

const settingsSections = [
  ['overview', 'Overview'],
  ['workspace', 'Workspace'],
  ['provider', 'Provider'],
  ['model', 'Model'],
  ['local-server', 'Local server'],
  ['context', 'Context and thinking'],
  ['chat', 'Chat display'],
  ['appearance', 'Appearance'],
  ['telegram', 'Telegram'],
  ['security', 'Security'],
  ['migration', 'Migration'],
  ['advanced', 'Advanced'],
  ['skills', 'Skills'],
  ['feedback', 'Help & Feedback'],
];

function renderSettingsNav(activeSection) {
  return settingsSections
    .map(
      ([id, label]) => `
        <button class="${activeSection === id ? 'active' : ''}" type="button" data-settings-section="${escapeAttribute(id)}">
          ${escapeHtml(label)}
        </button>
      `,
    )
    .join('');
}

function renderSettingsContent(state, activeSection, provider, metadata) {
  if (activeSection === 'overview') {
    return `
      <section class="settings-overview-grid">
        ${settingsSections
          .filter(([id]) => id !== 'overview')
          .map(
            ([id, label]) => `
              <button class="settings-overview-card" type="button" data-settings-section="${escapeAttribute(id)}">
                <strong>${escapeHtml(label)}</strong>
                <span>${escapeHtml(settingsSectionSummary(id, state, provider, metadata))}</span>
              </button>
            `,
          )
          .join('')}
      </section>
    `;
  }

  const title = settingsSections.find(([id]) => id === activeSection)?.[1] || 'Settings';
  return `
    <section class="settings-field-group active-settings-section">
      <div class="settings-group-title">
        <button class="button compact" type="button" data-settings-section="overview">Back</button>
        <div>
          <h3>${escapeHtml(title)}</h3>
          <p>${escapeHtml(settingsSectionSummary(activeSection, state, provider, metadata))}</p>
        </div>
      </div>
      <div class="panel-body form-grid two">
        ${renderSettingsSectionFields(state, activeSection, provider, metadata)}
      </div>
    </section>
  `;
}

function settingsSectionSummary(id, state, provider, metadata) {
  const summaries = {
    'workspace': `Current path: ${state.workspacePath}`,
    'provider': `${provider.label}; auth ${state.providerAuthMethod}`,
    'model': `${displayModelName(state.providerModel)}; ${metadata.contextWindow}`,
    'local-server': state.desktopState?.llamaServerPid
      ? `Running at ${state.desktopState.llamaServerEndpoint || 'localhost'}`
      : state.desktopState?.llamaServerInstalled
        ? 'Installed, stopped'
        : 'Binary not installed',
    'context': `${state.thinkingLevel} thinking; ${state.selectedContextAccess.length} context sources`,
    'chat': state.showThinkingInChat ? 'Reasoning visible in chat' : 'Reasoning hidden in chat',
    'appearance': `Language: ${state.uiLanguage}; accent ${state.accentColor || 'default red'}`,
    'telegram': state.selectedChannels.includes('telegram') ? 'Telegram selected' : 'Telegram off',
    'security': labelFor(securityProfiles, state.securityProfile),
    'migration': state.migrationResults
      ? 'Import complete'
      : state.migrationSources?.openclaw?.found
        ? `OpenClaw found — ${state.migrationSources.openclaw.item_count} item(s)`
        : state.migrationSources?.hermes?.found
          ? `Hermes found — ${state.migrationSources.hermes.item_count} item(s)`
          : 'No legacy agent detected',
    'advanced': `${labelFor(runtimeModes, state.runtimeMode)} runtime; fonts and diagnostics`,
  };
  return summaries[id] || 'Open a settings section.';
}

function renderSettingsSectionFields(state, activeSection, provider, metadata) {
  const availableAuthMethods = providerAuthMethods.filter((method) =>
    (provider.authMethods || ['api-key']).includes(method.id),
  );

  if (activeSection === 'workspace') {
    return `
      <label>
        Workspace
        <input id="settings-workspace" value="${escapeHtml(state.workspacePath)}" />
      </label>
      <div class="settings-inline-note">
        <strong>Default boundary</strong>
        <p>Argentum works inside this folder unless you approve a broader capability.</p>
      </div>
    `;
  }

  if (activeSection === 'provider') {
    return `
      <label>
        Provider
        <select id="settings-provider">
          ${providerCatalogTabs
            .map(
              (tab) => `
                <optgroup label="${escapeAttribute(tab.id === 'testing' ? 'Testing access' : tab.label)}">
                  ${providerPresets
                    .filter((item) => (item.access || 'testing') === tab.id)
                    .map(
                      (item) => `
                        <option value="${item.id}" data-provider-access="${escapeAttribute(tab.id)}" ${item.id === state.llmProvider ? 'selected' : ''}>${escapeHtml(item.label)}</option>
                      `,
                    )
                    .join('')}
                </optgroup>
              `,
            )
            .join('')}
        </select>
        <a class="provider-website-link inline" href="${escapeAttribute(provider.websiteUrl)}" data-open-external="${escapeAttribute(provider.websiteUrl)}">
          Open provider website <span data-icon="externalLink"></span>
        </a>
      </label>
      <label>
        Authorization method
        <select id="settings-provider-auth-method">
          ${availableAuthMethods
            .map(
              (method) => `
                <option value="${escapeAttribute(method.id)}" ${selected(state.providerAuthMethod, method.id)} ${method.disabled ? 'disabled' : ''}>${escapeHtml(method.label)}</option>
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
        API key
        <input id="settings-provider-api-key" type="password" value="${escapeAttribute(state.providerApiKey)}" placeholder="Paste a new key, or leave blank to keep saved key." autocomplete="new-password" />
        <small>${escapeHtml(provider.requiresKey ? 'Stored in workspace secrets, not YAML.' : 'Optional for LM Studio/local/custom endpoints unless that server requires authorization.')}</small>
      </label>
      ${renderSettingsOAuthPanel(state)}
    `;
  }

  if (activeSection === 'model') {
    const supportsThinking = (metadata.capabilities || []).some(
      (c) => String(c).toLowerCase().includes('reasoning') || String(c).toLowerCase().includes('thinking'),
    );
    return `
      <label>
        Model
        <select id="settings-provider-model">
          ${modelOptionsFor(provider, state.providerModel, state.providerAuthMethod)
            .map(
              (model) => `
                <option value="${escapeAttribute(model.id)}" ${selected(state.providerModel, model.id)}>${escapeHtml(model.label || displayModelName(model.id))}</option>
              `,
            )
            .join('')}
        </select>
      </label>
      <label>
        Other / custom model ID
        <input id="settings-provider-custom-model" value="${escapeAttribute(state.providerModel)}" placeholder="Exact model ID from the selected endpoint" />
        <small>Use this for local models exposed by LM Studio, HuggingFace downloads, or custom provider IDs.</small>
      </label>
      <div class="settings-inline-note">
        <strong>${escapeHtml(metadata.contextWindow)}</strong>
        <p>${escapeHtml(metadata.detail)} ${escapeHtml((metadata.capabilities || []).join(', '))}.</p>
      </div>
      <label>
        API style
        <select id="provider-api">
          <option value="openai" ${selected(state.providerApi, 'openai')}>OpenAI-compatible</option>
          <option value="anthropic" ${selected(state.providerApi, 'anthropic')}>Anthropic-compatible</option>
        </select>
      </label>
      ${supportsThinking
        ? `<div class="settings-inline-note">
            <strong>Thinking mode</strong>
            <p>${escapeHtml(provider.label)} supports extended reasoning. The thinking level (fast/balanced/deep) controls how much effort the model spends on internal reasoning before responding. Configure it in the "Context and thinking" settings section or in the chat composer.</p>
          </div>`
        : `<div class="settings-inline-note">
            <strong>Thinking mode</strong>
            <p>The current model does not support an externally-controlled thinking mode. The thinking level setting has no effect for this model.</p>
          </div>`
      }
    `;
  }

  if (activeSection === 'local-server') {
    const config = state.llamaServerConfig || {};
    const modelSource = config.modelSource || 'huggingface';
    const selectedPreset =
      llamaDownloadPresets.find((preset) => preset.id === config.modelPreset) ||
      llamaDownloadPresets[0];
    const presetOptions = llamaDownloadPresets
      .map(
        (preset) => `
          <option value="${escapeAttribute(preset.id)}" ${selected(config.modelPreset || selectedPreset.id, preset.id)}>
            ${escapeHtml(`${preset.label} (${preset.size})`)}
          </option>
        `,
      )
      .join('');
    return `
      <label>
        Model source
        <select id="settings-llama-model-source">
          <option value="huggingface" ${selected(modelSource === 'huggingface')}>Download from Hugging Face</option>
          <option value="file" ${selected(modelSource === 'file')}>Local GGUF file</option>
        </select>
        <small>Hugging Face uses llama.cpp's native <code>--hf-repo</code>/<code>--hf-file</code> download path. Local files may be selected outside the workspace only when they are explicit GGUF model files.</small>
      </label>
      ${
        modelSource === 'huggingface'
          ? `
            <label>
              Download preset
              <select id="settings-llama-model-preset">
                ${presetOptions}
                <option value="custom" ${selected(config.modelPreset === 'custom')}>Other / custom Hugging Face GGUF</option>
              </select>
              <small>${escapeHtml(selectedPreset.detail || 'Choose a GGUF preset that matches the machine.')}</small>
            </label>
            <label>
              Hugging Face repo
              <input id="settings-llama-hf-repo" value="${escapeAttribute(config.hfRepo || selectedPreset.repo)}" />
              <small>Format: <code>owner/model-GGUF[:quant]</code>. Example: <code>ggml-org/gemma-3-1b-it-GGUF:Q4_K_M</code>.</small>
            </label>
            <label>
              Hugging Face file
              <input id="settings-llama-hf-file" value="${escapeAttribute(config.hfFile || selectedPreset.file || '')}" />
              <small>Optional when the repo quant suffix is enough. Use an exact <code>.gguf</code> filename for deterministic downloads.</small>
            </label>
          `
          : `
            <label>
              Model path
              <div class="input-with-action">
                <input id="settings-llama-model-path" value="${escapeAttribute(config.modelPath || '')}" />
                <button class="button compact" id="choose-llama-model" type="button">Browse</button>
              </div>
              <small>Use a GGUF file. Workspace models still work, and explicit external GGUF files are allowed for llama.cpp only.</small>
            </label>
          `
      }
      <label>
        Endpoint host
        <input id="settings-llama-host" value="${escapeAttribute(config.host || '127.0.0.1')}" />
        <small>Restricted to localhost by the desktop bridge.</small>
      </label>
      <label>
        Port
        <input id="settings-llama-port" type="number" min="1" max="65535" value="${escapeAttribute(config.port || 8080)}" />
      </label>
      <label>
        Context size
        <input id="settings-llama-context-size" type="number" min="512" step="512" value="${escapeAttribute(config.contextSize || 8192)}" />
      </label>
      <label>
        GPU layers
        <input id="settings-llama-gpu-layers" type="number" min="0" step="1" value="${escapeAttribute(config.gpuLayers ?? 0)}" />
      </label>
      <label>
        Threads
        <input id="settings-llama-threads" type="number" min="0" step="1" value="${escapeAttribute(config.threads ?? 0)}" />
        <small>Use 0 to let llama.cpp choose.</small>
      </label>
      <label>
        Temperature
        <input id="settings-llama-temperature" type="number" min="0" max="2" step="0.05" value="${escapeAttribute(config.temperature ?? 0.7)}" />
      </label>
      <label>
        Top-p
        <input id="settings-llama-top-p" type="number" min="0" max="1" step="0.01" value="${escapeAttribute(config.topP ?? 0.95)}" />
      </label>
      <label>
        Repeat penalty
        <input id="settings-llama-repeat-penalty" type="number" min="0" max="2" step="0.01" value="${escapeAttribute(config.repeatPenalty ?? 1.1)}" />
      </label>
      <label>
        Batch size
        <input id="settings-llama-batch-size" type="number" min="1" step="1" value="${escapeAttribute(config.batchSize ?? 1024)}" />
      </label>
      <label>
        Micro batch
        <input id="settings-llama-ubatch-size" type="number" min="1" step="1" value="${escapeAttribute(config.ubatchSize ?? 256)}" />
      </label>
      <label>
        Parallel slots
        <input id="settings-llama-parallel-slots" type="number" min="1" step="1" value="${escapeAttribute(config.parallelSlots ?? 1)}" />
      </label>
      <label>
        CPU MoE layers
        <input id="settings-llama-cpu-moe" type="number" min="0" step="1" value="${escapeAttribute(config.cpuMoe ?? 22)}" />
      </label>
      <label>
        Idle timeout
        <input id="settings-llama-timeout" type="number" min="0" step="1" value="${escapeAttribute(config.timeout ?? 0)}" />
        <small>0 disables llama.cpp idle timeout for slow local generations.</small>
      </label>
      <label>
        KV cache K
        <input id="settings-llama-cache-type-k" value="${escapeAttribute(config.cacheTypeK || 'f16')}" />
      </label>
      <label>
        KV cache V
        <input id="settings-llama-cache-type-v" value="${escapeAttribute(config.cacheTypeV || 'f16')}" />
      </label>
      <label class="check-card compact-toggle ${config.flashAttention !== false ? 'active' : ''}">
        <span class="check-card-head">
          <input type="checkbox" data-llama-boolean="flashAttention" ${checked([config.flashAttention === false ? '' : 'flashAttention'], 'flashAttention')} />
          <span><em>GPU</em><strong>Flash attention</strong></span>
        </span>
      </label>
      <label class="check-card compact-toggle ${config.noMmap !== false ? 'active' : ''}">
        <span class="check-card-head">
          <input type="checkbox" data-llama-boolean="noMmap" ${checked([config.noMmap === false ? '' : 'noMmap'], 'noMmap')} />
          <span><em>Memory</em><strong>No mmap</strong></span>
        </span>
      </label>
      <label class="check-card compact-toggle ${config.mlock !== false ? 'active' : ''}">
        <span class="check-card-head">
          <input type="checkbox" data-llama-boolean="mlock" ${checked([config.mlock === false ? '' : 'mlock'], 'mlock')} />
          <span><em>Memory</em><strong>Lock model in RAM</strong></span>
        </span>
      </label>
      <label class="check-card compact-toggle ${config.jinja !== false ? 'active' : ''}">
        <span class="check-card-head">
          <input type="checkbox" data-llama-boolean="jinja" ${checked([config.jinja === false ? '' : 'jinja'], 'jinja')} />
          <span><em>Prompting</em><strong>Jinja templates</strong></span>
        </span>
      </label>
      <div class="settings-inline-note">
        <strong>${state.desktopState?.llamaServerInstalled ? 'Binary found' : 'Binary not installed'}</strong>
        <p>${escapeHtml(state.desktopState?.llamaServerInstalled ? `Endpoint: ${state.desktopState?.llamaServerEndpoint || 'http://127.0.0.1:8080/v1'}` : 'Install from the Local Server page, select the optional llama.cpp checkbox in setup.exe, set LLAMA_SERVER_BIN, or place llama-server in workspace/bin.')}</p>
      </div>
    `;
  }

  if (activeSection === 'context') {
    return `
      <label>
        Thinking level
        <select id="thinking-level">
          ${thinkingLevels
            .map(
              (item) => `
                <option value="${escapeAttribute(item.id)}" ${selected(state.thinkingLevel, item.id)}>${escapeHtml(item.label)} - ${escapeHtml(item.effort)}</option>
              `,
            )
            .join('')}
        </select>
      </label>
      ${renderContextAccessCards(state)}
    `;
  }

  if (activeSection === 'chat') {
    return `
      <label class="check-card compact-toggle ${state.showThinkingInChat ? 'active' : ''}">
        <span class="check-card-head">
          <input id="settings-show-thinking-chat" type="checkbox" ${state.showThinkingInChat ? 'checked' : ''} />
          <span><em>Display</em><strong>Show reasoning blocks</strong></span>
        </span>
        <p>Show provider text inside &lt;think&gt; or &lt;reasoning&gt; in a collapsed panel.</p>
      </label>
      <label>
        Interface font
        <select id="settings-ui-font">
          ${fontOptions.ui
            .map(
              (font) => `
                <option value="${escapeAttribute(font.css)}" ${selected(state.uiFontFamily, font.css)}>${escapeHtml(font.label)}</option>
              `,
            )
            .join('')}
        </select>
      </label>
      <label>
        Code and terminal font
        <select id="settings-code-font">
          ${fontOptions.mono
            .map(
              (font) => `
                <option value="${escapeAttribute(font.css)}" ${selected(state.codeFontFamily, font.css)}>${escapeHtml(font.label)}</option>
              `,
            )
            .join('')}
        </select>
      </label>
      <label>
        User avatar
        <div class="input-with-action">
          <input value="${escapeAttribute(state.userAvatarPath || '')}" readonly placeholder="No image selected" />
          <button class="button compact" id="choose-user-avatar" type="button">Choose</button>
          <button class="button ghost compact" id="clear-user-avatar" type="button" ${state.userAvatarPath ? '' : 'disabled'}>Clear</button>
        </div>
        <small>Optional local image used for your chat avatar. The path stays in desktop UI preferences.</small>
      </label>
    `;
  }

  if (activeSection === 'appearance') {
    return `
      <label>
        Language
        <select id="settings-language">
          ${SUPPORTED_LOCALES
            .map(
              (locale) => `
                <option value="${escapeAttribute(locale.code)}" ${state.uiLanguage === locale.code ? 'selected' : ''}>${escapeHtml(locale.label)}</option>
              `,
            )
            .join('')}
        </select>
        <small>Changes the interface language. More languages will be added in future releases.</small>
      </label>
      ${renderAppearanceFields(state)}
    `;
  }

  if (activeSection === 'telegram') {
    return `
      <label class="check-card compact-toggle ${state.showThinkingInTelegram ? 'active' : ''}">
        <span class="check-card-head">
          <input id="settings-show-thinking-telegram" type="checkbox" ${state.showThinkingInTelegram ? 'checked' : ''} />
          <span><em>Telegram</em><strong>Send reasoning blocks</strong></span>
        </span>
        <p>Keep this off unless you want Telegram replies to include separated reasoning context.</p>
      </label>
      <label>
        Telegram bot token
        <input id="telegram-token" type="password" value="${escapeAttribute(state.telegramToken)}" placeholder="Paste token to save or replace" autocomplete="new-password" />
      </label>
      <label>
        Allowed users/chats
        <input id="telegram-allowlist" value="${escapeAttribute(state.telegramAllowlist)}" placeholder="Comma-separated user or chat IDs" />
      </label>
      <button class="button" data-run-action="telegram-status">Test Telegram status</button>
    `;
  }

  if (activeSection === 'security') {
    return `
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
        Your name
        <input id="profile-user-name" value="${escapeAttribute(state.userName)}" placeholder="Example: AG" />
      </label>
      <label>
        Agent name
        <input id="profile-agent-name" value="${escapeAttribute(state.agentName)}" placeholder="Argentum" />
      </label>
      <label class="full-span">
        System prompt
        <textarea id="profile-purpose" rows="4" placeholder="Optional instructions that guide how Argentum behaves. Keep it short and focused.">${escapeHtml(state.systemPrompt || '')}</textarea>
        <small>Shown to the model on every request. Keep it brief.</small>
      </label>
    `;
  }

  if (activeSection === 'advanced') {
    return `
      <label>
        Runtime mode
        <select id="settings-runtime-mode">
          ${runtimeModes
            .map(
              (item) => `
                <option value="${item.id}" ${selected(state.runtimeMode, item.id)}>${escapeHtml(item.label)}</option>
              `,
            )
            .join('')}
        </select>
      </label>
      <div class="settings-inline-note">
        <strong>${escapeHtml(labelFor(runtimeModes, state.runtimeMode))}</strong>
        <p>${escapeHtml(labelFor(runtimeModes, state.runtimeMode) === 'Desktop app' ? 'GUI first, CLI tools available. Chat, settings, diagnostics, and gateway controls remain in the GUI.' : 'Terminal-first. Desktop settings still save to the workspace config used by argentum commands.')}</p>
      </div>
      ${renderSettingsOAuthPanel(state)}
    `;
  }

  if (activeSection === 'skills') {
    // Installed skills come from Rust state; catalog skills are hardcoded
    const installedNames = new Set(
      (state.installedSkills || []).map((s) => s.name),
    );
    const anthropicSkills = (state.skillsCatalog?.anthropic || []).map(normalizeCatalog);
    const codexSkills = (state.skillsCatalog?.codex || []).map(normalizeCatalog);
    const installedList = (state.installedSkills || []).map((s) => ({
      ...normalizeInstalled(s),
      isInstalled: true,
    }));
    const activeTab = state.skillsTab || 'argentum';
    const searchQuery = state.skillsSearch || '';
    const filterCategory = state.skillsCategory || 'all';

    const filterByCategory = (skills) => {
      if (filterCategory === 'all') return skills;
      return skills.filter((s) => s.category === filterCategory);
    };

    const filterBySearch = (skills) => {
      if (!searchQuery.trim()) return skills;
      const q = searchQuery.toLowerCase();
      return skills.filter(
        (s) =>
          s.name.toLowerCase().includes(q) ||
          s.description.toLowerCase().includes(q) ||
          (s.tags || []).some((t) => t.toLowerCase().includes(q)),
      );
    };

    // Normalize installed skills to have the same shape as catalog skills
    const normalizeInstalled = (s) => ({
      name: s.name,
      description: s.description || `Custom skill in your local skills directory`,
      source: 'installed',
      tags: s.tags || [],
      url: s.url || null,
      category: s.category || 'general',
      builtinNote: s.builtinNote || null,
    });

    const normalizeCatalog = (s) => ({ ...s, isInstalled: installedNames.has(s.name) });

    const renderSkillCard = (skill) => {
      const safeSource = skill.source || 'installed';
      const safeDesc = skill.description || '';
      const isArgentumBuiltin = safeSource === 'argentum';
      const badgeLabel = isArgentumBuiltin ? 'Built-in' : escapeHtml(safeSource);
      const badgeClass = isArgentumBuiltin ? 'builtin' : safeSource;
      return `
        <div class="skill-card">
          <div class="skill-card-header">
            <div class="skill-card-name-row">
              <span class="skill-card-name">${escapeHtml(skill.name)}</span>
              <span class="skill-source-badge source-${escapeAttribute(badgeClass)}">${badgeLabel}</span>
            </div>
            <div class="skill-card-tags">
              ${(skill.tags || []).slice(0, 3).map((t) => `<span class="skill-tag">${escapeHtml(t)}</span>`).join('')}
            </div>
          </div>
          ${safeDesc ? `<p class="skill-card-description">${escapeHtml(safeDesc.slice(0, 180))}${safeDesc.length > 180 ? '…' : ''}</p>` : ''}
          ${skill.builtinNote ? `<p class="skill-builtin-note"><em>${escapeHtml(skill.builtinNote)}</em></p>` : ''}
          ${isArgentumBuiltin
            ? `<p class="skill-builtin-note"><em>Part of the standard Argentum installation — always available.</em></p>`
            : `<div class="skill-card-actions">
                ${safeSource === 'installed'
                  ? `<button type="button" class="button compact uninstall-skill-btn" data-skill-name="${escapeAttribute(skill.name)}" title="Uninstall">Uninstall</button>`
                  : `<button type="button" class="button primary compact install-skill-btn" data-skill-name="${escapeAttribute(skill.name)}" data-skill-source="${escapeAttribute(safeSource)}" title="Install">Install</button>`
                }
                ${skill.url ? `<a href="${escapeAttribute(skill.url)}" class="button compact" data-open-external="${escapeAttribute(skill.url)}" target="_blank" rel="noopener">View on GitHub</a>` : ''}
              </div>`
          }
        </div>`;
    };

    const renderTab = (tabId, label, skills, count) => `
      <button type="button" class="skills-tab-btn ${activeTab === tabId ? 'active' : ''}" data-skills-tab="${escapeAttribute(tabId)}">
        ${escapeHtml(label)} <span class="skills-tab-count">${count}</span>
      </button>`;

    const anthropicFiltered = filterBySearch(filterByCategory(anthropicSkills));
    const codexFiltered = filterBySearch(filterByCategory(codexSkills));
    const installedFiltered = filterBySearch(filterByCategory(installedList));
    const argentumSkills = ARGENTUM_BUNDLED_SKILLS.map(normalizeCatalog);
    const argentumFiltered = filterBySearch(filterByCategory(argentumSkills));
    const tabContent =
      activeTab === 'anthropic'
        ? anthropicFiltered
        : activeTab === 'codex'
          ? codexFiltered
          : activeTab === 'argentum'
            ? argentumFiltered
            : installedFiltered;

    const totalCount =
      activeTab === 'anthropic'
        ? anthropicSkills.length
        : activeTab === 'codex'
          ? codexSkills.length
          : activeTab === 'argentum'
            ? argentumSkills.length
            : installedList.length;

    return `
      <div class="skills-section">
        <div class="skills-section-header">
          <div>
            <h3>Skills</h3>
            <p class="muted-line">Browse Argentum's built-in skills and install additional skills from Anthropic and Codex to extend Argentum's capabilities with specialized knowledge and workflows.</p>
          </div>
        </div>
        <div class="skills-toolbar">
          <div class="skills-tabs">
            ${renderTab('argentum', 'Built-in', argentumSkills, argentumSkills.length)}
            ${renderTab('anthropic', 'Anthropic', anthropicSkills, anthropicSkills.length)}
            ${renderTab('codex', 'Codex', codexSkills, codexSkills.length)}
            ${renderTab('installed', 'Installed', installedList, installedList.length)}
          </div>
          <div class="skills-toolbar-right">
            <select class="select compact skills-category-filter" title="Filter by category">
              <option value="all" ${filterCategory === 'all' ? 'selected' : ''}>All categories</option>
              <option value="document" ${filterCategory === 'document' ? 'selected' : ''}>Documents</option>
              <option value="design" ${filterCategory === 'design' ? 'selected' : ''}>Design</option>
              <option value="code" ${filterCategory === 'code' ? 'selected' : ''}>Code</option>
              <option value="devops" ${filterCategory === 'devops' ? 'selected' : ''}>DevOps</option>
              <option value="security" ${filterCategory === 'security' ? 'selected' : ''}>Security</option>
              <option value="ai" ${filterCategory === 'ai' ? 'selected' : ''}>AI & APIs</option>
              <option value="collaboration" ${filterCategory === 'collaboration' ? 'selected' : ''}>Collaboration</option>
              <option value="testing" ${filterCategory === 'testing' ? 'selected' : ''}>Testing</option>
              <option value="deployment" ${filterCategory === 'deployment' ? 'selected' : ''}>Deployment</option>
              <option value="general" ${filterCategory === 'general' ? 'selected' : ''}>General</option>
            </select>
            <input
              type="search"
              class="input compact skills-search"
              placeholder="Search skills…"
              value="${escapeAttribute(searchQuery)}"
            />
          </div>
        </div>
        ${tabContent.length === 0
          ? `<div class="skills-empty">
               <p class="muted-line">${totalCount === 0 ? 'No skills in this tab yet.' : 'No skills match your search.'}</p>
               ${activeTab === 'installed' && installedList.length === 0 ? '<p class="muted-line">Install skills from the Anthropic or Codex tabs above.</p>' : ''}
             </div>`
          : `<div class="skills-grid">${tabContent.map(renderSkillCard).join('')}</div>`
        }
        <div class="skills-footer-note">
          <p class="muted-line">Skills are loaded from <code>~/.openclaw/workspace/skills/</code> and <code>%LOCALAPPDATA%/argentum/skills/</code>. Restart Argentum after installing or uninstalling.</p>
        </div>
      </div>
    `;
  }

  if (activeSection === 'feedback') {
    const version = '0.0.9';
    const bugUrl =
      'https://github.com/AG064/argentum/issues/new?title=%5BBug%5D%20Brief%20description&labels=bug&body=' +
      encodeURIComponent(
        '## Version\n' + version + '\n\n## Steps to reproduce\n1. \n2. \n3. \n\n## Expected behavior\n\n\n## Actual behavior\n\n\n## Environment\n- OS: \n- Workspace: ' +
        (state.workspacePath || 'not set') +
        '\n',
      );
    const featureUrl =
      'https://github.com/AG064/argentum/discussions/new?category=ideas&title=%5BFeature%5D%20Brief%20description&body=' +
      encodeURIComponent(
        '## Problem you want to solve\n\n\n## Suggested solution\n\n\n## Alternatives considered\n\n',
      );
    return `
      <div class="feedback-section">
        <div class="feedback-header">
          <h3>Help & Feedback</h3>
          <p class="muted-line">Found something broken or have an idea? Let us know.</p>
        </div>
        <div class="feedback-cards">
          <div class="feedback-card">
            <div class="feedback-card-icon" aria-hidden="true">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 9v4m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
              </svg>
            </div>
            <div>
              <h4>Report a bug</h4>
              <p class="muted-line">Something is broken or not working as expected.</p>
            </div>
          </div>
          <div class="feedback-actions">
            <button type="button" class="button primary" id="feedback-bug" data-feedback-url="${escapeAttribute(bugUrl)}">
              Open bug report
            </button>
          </div>
        </div>
        <div class="feedback-cards">
          <div class="feedback-card">
            <div class="feedback-card-icon" aria-hidden="true">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"/>
              </svg>
            </div>
            <div>
              <h4>Request a feature</h4>
              <p class="muted-line">Suggest a new capability or improvement.</p>
            </div>
          </div>
          <div class="feedback-actions">
            <button type="button" class="button primary" id="feedback-feature" data-feedback-url="${escapeAttribute(featureUrl)}">
              Open feature request
            </button>
          </div>
        </div>
        <div class="feedback-info-box">
          <p class="muted-line">Bug reports and feature requests open in your browser on GitHub. Your version (${version}) and workspace path are pre-filled to save time.</p>
        </div>
        <div class="feedback-email-hint">
          <p class="muted-line">For private security issues, email <a href="mailto:agdroke064@gmail.com" data-open-external="mailto:agdroke064@gmail.com">agdroke064@gmail.com</a> directly.</p>
        </div>
      </div>
    `;
  }

  if (activeSection === 'migration') {
    const openclaw = state.migrationSources?.openclaw;
    const hermes = state.migrationSources?.hermes;
    const source = openclaw?.found ? openclaw : hermes?.found ? hermes : null;
    const sourceName = openclaw?.found ? 'OpenClaw' : 'Hermes';

    if (state.migrationResults) {
      const results = state.migrationResults;
      const ok = results.filter((r) => r.status === 'ok').length;
      const err = results.filter((r) => r.status === 'error').length;
      return `
        <div class="migration-result-card full-span">
          <h4>Migration complete</h4>
          <p>${ok} item(s) migrated${err > 0 ? `, ${err} error(s)` : ''}.</p>
          ${err > 0
            ? `<p class="muted-line">Check the list below for details on failed items.</p>
               <ul>${results.filter((r) => r.status === 'error').map((r) => `<li>${escapeHtml(r.id)}: ${escapeHtml(r.message)}</li>`).join('')}</ul>`
            : ''}
        </div>
      `;
    }

    if (!source) {
      return `
        <div class="settings-inline-note full-span">
          <strong>No legacy agent detected</strong>
          <p>Argentum scanned <code>~/.openclaw/</code> and <code>~/.hermes/</code> but found no previous installation.</p>
          <button class="button" id="settings-rescan-migration">Scan again</button>
        </div>
      `;
    }

    const items = source.items || [];
    const totalSize = source.size_bytes || 0;

    return `
      <div class="full-span migration-card" id="settings-migration-card">
        <div class="migration-card-header">
          <h4>Import from ${sourceName}</h4>
          <span class="muted-line">${items.length} item(s) · ${formatBytes(source.size_bytes)}</span>
        </div>
        <ul class="migration-item-list">
          ${items.map((item) => `
            <li>
              <span>${escapeHtml(item.label)}</span>
              <span class="muted-line">${escapeHtml(item.description)}</span>
            </li>
          `).join('')}
        </ul>
        <div class="migration-card-actions">
          <button type="button" class="button primary" id="settings-do-migration">
            Import into Argentum
          </button>
        </div>
      </div>
    `;
  }

  return '';
}

// ─── Appearance fields (shared between appearance section and overview) ───────────

function renderAppearanceFields(state) {
  const presetColors = [
    { id: 'e23b3b', label: 'Default Red' },
    { id: '3b82f6', label: 'Blue' },
    { id: '22c55e', label: 'Green' },
    { id: 'a855f7', label: 'Purple' },
    { id: 'f59e0b', label: 'Amber' },
    { id: 'ec4899', label: 'Pink' },
    { id: '06b6d4', label: 'Cyan' },
    { id: '6366f1', label: 'Indigo' },
  ];
  return `
    <label>
      Accent color
      <div class="accent-color-picker">
        ${presetColors
          .map(
            (color) => `
              <button
                type="button"
                class="accent-color-swatch ${state.accentColor === color.id || (!state.accentColor && color.id === 'e23b3b') ? 'active' : ''}"
                data-accent-color="${color.id}"
                style="background:#${color.id}"
                aria-label="${escapeAttribute(color.label)}"
                title="${escapeAttribute(color.label)}"
              ></button>
            `,
          )
          .join('')}
        <input
          type="color"
          id="settings-accent-custom"
          value="${state.accentColor || '#e23b3b'}"
          class="accent-color-input"
          aria-label="Custom accent color"
        />
      </div>
      <small>Choose a preset or pick any color. The Argentum logo stays red.</small>
    </label>
    <label class="check-card compact-toggle ${state.highContrastMode ? 'active' : ''}">
      <span class="check-card-head">
        <input id="settings-high-contrast" type="checkbox" ${state.highContrastMode ? 'checked' : ''} />
        <span><em>Accessibility</em><strong>High contrast mode</strong></span>
      </span>
      <p>Increase contrast for better visibility. Affects text and UI borders.</p>
    </label>
  `;
}

function renderSettingsOAuthPanel(state) {
  const oauth = state.codexOAuth || {};
  const provider = currentProvider(providerPresets, state);
  if (!(provider.authMethods || []).includes('browser-account')) return '';

  const verificationUrl = oauth.verificationUrl || 'https://auth.openai.com/codex/device';

  return `
    <div class="panel-body oauth-panel ${state.providerAuthMethod === 'browser-account' ? 'active' : ''}">
      <div>
        <span class="pill">OpenAI/Codex OAuth</span>
        <h3>Browser Account Authorization</h3>
        <p>Use this only for OpenAI/Codex mode. Start authorization, approve it in your browser, then complete it here. API key auth remains available above for live Platform API chat.</p>
      </div>
      <div class="oauth-actions">
        <button class="button" id="start-codex-oauth">Start OpenAI/Codex authorization</button>
        <button class="button primary" id="complete-codex-oauth">Complete authorization</button>
      </div>
      <div class="oauth-code-grid">
        <div>
          <span>Verification page</span>
          <a href="${escapeAttribute(verificationUrl)}" data-open-external="${escapeAttribute(verificationUrl)}">
            ${escapeHtml(verificationUrl)}
            <span data-icon="externalLink"></span>
          </a>
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
    message: state.desktopState?.configExists
      ? 'Configuration exists.'
      : 'Configuration has not been saved yet.',
  }),
  render: (state) => {
    const checks = [
      [
        'Config',
        state.desktopState?.configExists
          ? state.desktopState.configPath || state.savedConfigPath
          : 'Missing until setup is saved',
        state.desktopState?.configExists,
      ],
      [
        'Data directory',
        formatFound(state.desktopState?.dataExists),
        state.desktopState?.dataExists,
      ],
      [
        'Logs directory',
        formatFound(state.desktopState?.logsExists),
        state.desktopState?.logsExists,
      ],
      [
        'Gateway',
        state.desktopState?.gatewayPid ? `PID ${state.desktopState.gatewayPid}` : 'Stopped',
        true,
      ],
      [
        'Workspace boundary',
        `${state.workspacePath} (${formatWorkspaceHealth()})`,
        Boolean(state.workspacePath),
      ],
      ['Tauri bridge', window.__TAURI__ ? 'Connected' : 'Preview mode', true],
      ['Provider', state.apiTest.message, state.apiTest.status !== 'error'],
      [
        'Telegram',
        state.desktopState?.telegramDiagnostics?.lastResponseStatus ||
          (state.selectedChannels.includes('telegram')
            ? 'Configured, waiting for bot status'
            : 'Not selected'),
        !state.desktopState?.telegramDiagnostics?.lastError,
      ],
      ['Capability broker', labelFor(securityProfiles, state.securityProfile), true],
      ['Secrets', 'Stored outside YAML', true],
    ];
    const estimatedTokens = estimateRuntimeContextTokens(state, state.draftMessage);
    const usage = state.usageSnapshot;
    const providerUsage = usage?.summary
      ? usage.summary
      : usage
        ? `${usage.requestRemaining || '?'} requests / ${usage.tokenRemaining || '?'} tokens left`
        : 'Not reported yet';
    const resetCadence =
      usage?.requestResetCadence ||
      usage?.tokenResetCadence ||
      usage?.resetCadence ||
      'Provider did not report a reset cadence.';
    const providerReset = usage?.requestReset || usage?.tokenReset || 'Unknown';

    return `
      ${renderNotifications()}
      ${renderProductTabShell(
        'diagnostics',
        `
          ${renderCompactPageHeader({
            kicker: 'Diagnostics',
            title: 'Health checks and repair suggestions',
            detail:
              'Checks are modular. A failed provider or gateway check does not stop Chat or Security from opening.',
            tone: 'diagnostics',
            stats: [
              { label: 'Config', value: state.setupComplete ? 'Saved' : 'Pending' },
              { label: 'Gateway', value: state.desktopState?.gatewayPid ? 'Running' : 'Stopped' },
              { label: 'Modules', value: 'Contained' },
            ],
            actions: renderCommandDock([{ label: 'Refresh', refresh: true, primary: true }]),
          })}
          <div class="diagnostics-matrix">
            <section class="panel glass-panel">
              <div class="panel-header">
                <span class="pill">Usage</span>
                <h3>Usage Snapshot</h3>
                <p>Live counters are local to the desktop app and update as chat, gateway, diagnostics, and logs are used.</p>
              </div>
              <div class="panel-body usage-grid">
                <div><span>Estimated chat context</span><strong>${estimatedTokens.toLocaleString()} tokens</strong></div>
                <div><span>Provider rate limits</span><strong>${escapeHtml(providerUsage)}</strong></div>
                <div><span>Provider reset</span><strong>${escapeHtml(providerReset)}</strong><small>${escapeHtml(resetCadence)}</small></div>
                <div><span>Terminal entries</span><strong>${state.terminalEntries.length}</strong></div>
                <div><span>Notifications stored</span><strong>${state.notificationHistory.length}</strong></div>
                <div><span>Gateway</span><strong>${state.desktopState?.gatewayPid ? `PID ${state.desktopState.gatewayPid}` : 'Stopped'}</strong></div>
              </div>
              ${renderUsageWindows(usage)}
            </section>
            <section class="panel glass-panel">
              <div class="panel-header split-header">
                <div>
                  <span class="pill">Doctor</span>
                  <h3>Health Checks</h3>
                </div>
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
          </div>
        `,
      )}
    `;
  },
};

function renderContextAccessCards(state) {
  return contextAccessOptions
    .map(
      (option) => `
        <label class="check-card ${state.selectedContextAccess.includes(option.id) ? 'active' : ''}">
          <span class="check-card-head">
            <input type="checkbox" data-context-access="${escapeAttribute(option.id)}" ${checked(state.selectedContextAccess, option.id)} />
            <span>
              <em>${escapeHtml(option.status)}</em>
              <strong>${escapeHtml(option.label)}</strong>
            </span>
          </span>
          <p>${escapeHtml(option.detail)}</p>
        </label>
      `,
    )
    .join('');
}

const updateModule = {
  id: 'update',
  title: 'Updates',
  eyebrow: 'New version',
  render(state) {
    const currentVersion = APP_VERSION;
    const newVersion = state.updateVersion || currentVersion;
    const isUpdate =
      state.updateAvailable && state.updateVersion && state.updateVersion !== currentVersion;

    return `
      <section class="settings-field-group">
        <div class="settings-group-title">
          <div>
            <h3>Argentum Updates</h3>
            <p>${isUpdate ? `Version ${escapeHtml(newVersion)} is available` : 'You are on the latest version'}</p>
          </div>
        </div>
        <div class="panel-body form-grid two">
          <label class="full-span">
            <span class="pill ${isUpdate ? 'warn' : 'ok'}">${isUpdate ? 'Update available' : 'Up to date'}</span>
            <p>Current version: <strong>${escapeHtml(currentVersion)}</strong></p>
            ${
              isUpdate
                ? `<p>A newer version <strong>${escapeHtml(newVersion)}</strong> is available for download.</p>`
                : '<p>You are running the latest Argentum version.</p>'
            }
          </label>
          ${
            isUpdate
              ? `
            <div class="full-span update-actions">
              ${
                state.updateDownloading
                  ? `
                <div class="update-progress">
                  <div class="progress-bar">
                    <div class="progress-fill" style="width:${state.updateProgress}%"></div>
                  </div>
                  <p>Downloading... ${state.updateProgress}%</p>
                </div>
              `
                  : `
                <button class="button primary" id="download-update" ${state.updateDownloading ? 'disabled' : ''}>
                  Download ${escapeHtml(newVersion)}
                </button>
              `
              }
              ${state.updateError ? `<p class="update-error">${escapeHtml(state.updateError)}</p>` : ''}
            </div>
          `
              : `
            <div class="full-span">
              <button class="button" id="check-for-updates">Check for updates</button>
            </div>
          `
          }
          <div class="full-span settings-inline-note">
            <strong>Automatic updates</strong>
            <p>Argentum checks for new versions on startup and displays this panel when an update is available.</p>
          </div>
        </div>
      </section>
    `;
  },
};

export const modules = {
  'onboarding': onboardingModule,
  'chat': chatModule,
  'gateway': gatewayModule(),
  'local-server': localServerModule(),
  'logs': logsModule,
  'security': securityModule,
  'pc-stats': pcStatsModule,
  'settings': settingsModule,
  'diagnostics': diagnosticsModule,
  'update': updateModule,
};
