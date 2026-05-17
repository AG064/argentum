import {
  contextAccessOptions,
  fontOptions,
  modelMetadata,
  providerAuthMethods,
  providerCatalogTabs,
  providerPresets,
  runtimeModes,
  securityProfiles,
  thinkingLevels,
} from './constants.js';
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
  escapeAttribute,
  escapeHtml,
  estimateRuntimeContextTokens,
  labelFor,
  modelMetadataFor,
  modelOptionsFor,
  selected,
} from './utils.js';

function terminalPreview(state, filter = '') {
  const entries = terminalEntriesForDisplay(filter);

  const actionOutput =
    entries.length === 0
      ? ''
      : entries
          .map((entry) => `$ ${entry.command}\n${entry.output}`)
          .join('\n\n');
  const logOutput = filter === 'gateway' ? state.desktopState?.gatewayLogPreview || '' : '';

  return [actionOutput, logOutput]
    .filter((part) => part && part !== 'No entries yet.')
    .join('\n\n')
    .trim() || 'No action output yet.';
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
    </div>
  `;
}

function renderProductHero({ kicker, title, detail, stats = [], actions = '', tone = '', statsClass = '' }) {
  return `
    <section class="product-hero ${escapeAttribute(tone)}">
      <div class="product-hero-copy">
        <span class="eyebrow">${escapeHtml(kicker)}</span>
        <h2>${escapeHtml(title)}</h2>
        <p>${escapeHtml(detail)}</p>
      </div>
      <div class="product-hero-status" aria-hidden="true">
        <span></span>
        <small>${escapeHtml(tone === 'online' ? 'Healthy' : tone === 'security' ? 'Guarded' : 'Ready')}</small>
      </div>
      <div class="product-hero-stats ${escapeAttribute(statsClass)}">
        ${stats
          .map(
            (stat) => `
              <div class="glass-panel stat-tile">
                <span>${escapeHtml(stat.label)}</span>
                <strong>${escapeHtml(stat.value)}</strong>
                ${stat.detail ? `<small>${escapeHtml(stat.detail)}</small>` : ''}
              </div>
            `,
          )
          .join('')}
      </div>
      ${actions ? `<div class="section-command-dock">${actions}</div>` : ''}
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
            ${renderProductHero({
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
            <div class="product-content-grid gateway-grid">
              <section class="panel gateway-actions-panel glass-panel">
                <div class="panel-header">
                  <span class="pill">Whitelisted</span>
                  <h3>Allowed gateway commands</h3>
                  <p>These are the only gateway commands exposed to the GUI. Output, PID, health URL, and failures appear in the terminal panel.</p>
                </div>
                ${renderActionCards('gateway')}
              </section>
              <section class="panel terminal-panel gateway-terminal-shell glass-panel">
                <div class="panel-header split-header">
                  <div>
                    <span class="pill">Terminal</span>
                    <h3>Gateway Output</h3>
                  </div>
                  <button class="button" data-run-action="gateway-status">Refresh</button>
                </div>
                <div class="terminal-body">
                  <article class="terminal-entry info">
                    <pre>${escapeHtml(terminalPreview(state, 'gateway'))}</pre>
                  </article>
                </div>
              </section>
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
    message: state.desktopState?.logsExists ? 'Logs directory exists.' : 'Logs appear after setup or runtime activity.',
  }),
  render: (state) => `
    ${renderNotifications()}
    ${renderProductTabShell(
      'logs',
      `
        ${renderProductHero({
          kicker: 'Activity logs',
          title: 'Trace actions without exposing secrets',
          detail:
            'Logs, audit entries, and command output stay readable from the app while sensitive lines are redacted and high-signal events remain searchable.',
          tone: 'logs',
          stats: [
            { label: 'Gateway', value: state.desktopState?.gatewayPid ? `PID ${state.desktopState.gatewayPid}` : 'Stopped' },
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
        ${renderProductHero({
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
  label: 'PC Statistics',
  validate: () => '',
  healthCheck: (state) => ({
    status: state.desktopState?.systemStats ? 'ok' : 'pending',
    message: state.desktopState?.systemStats
      ? 'System statistics are available from the desktop bridge.'
      : 'System statistics need the installed desktop app.',
  }),
  render: (state) => {
    const stats = state.desktopState?.systemStats || {};
    const disks = Array.isArray(stats.disks) ? stats.disks : [];
    const memoryUsed = Number(stats.memoryUsedBytes || 0);
    const memoryTotal = Number(stats.memoryTotalBytes || 0);
    const diskUsed = Number(stats.diskTotalBytes || 0) - Number(stats.diskAvailableBytes || 0);
    const networkTotal = Number(stats.networkReceivedBytes || 0) + Number(stats.networkTransmittedBytes || 0);
    const collectedAt = stats.collectedAt
      ? new Date(Number(stats.collectedAt) * 1000).toLocaleString()
      : 'Refresh to collect';

    return `
      ${renderNotifications()}
      ${renderProductTabShell(
        'pc-stats',
        `
          ${renderProductHero({
            kicker: 'System dashboard',
            title: 'PC statistics',
            detail:
              'Local CPU, memory, disk, network, uptime, and temperature snapshots are read from the desktop bridge. No provider or external API is required.',
            tone: 'diagnostics',
            stats: [
              { label: 'CPU', value: formatPercent(stats.cpuUsagePercent), detail: `${stats.cpuCores || 0} logical cores` },
              { label: 'Memory', value: formatPercent(stats.memoryUsedPercent), detail: `${formatBytes(memoryUsed)} / ${formatBytes(memoryTotal)}` },
              { label: 'Disk', value: formatPercent(stats.diskUsedPercent), detail: `${formatBytes(diskUsed)} used` },
            ],
            actions: renderCommandDock([{ label: 'Refresh statistics', refresh: true, primary: true }]),
          })}
          <div class="pc-stats-layout">
            <section class="panel glass-panel pc-stat-overview">
              <div class="panel-header split-header">
                <div>
                  <span class="pill">Live snapshot</span>
                  <h3>Resource usage</h3>
                </div>
                <button class="button" data-refresh-state="true">Refresh</button>
              </div>
              <div class="pc-stat-grid">
                <article class="pc-stat-card">
                  <span>CPU load</span>
                  <strong>${formatPercent(stats.cpuUsagePercent)}</strong>
                  ${renderStatBar(stats.cpuUsagePercent, 'CPU load')}
                  <small>${escapeHtml(stats.cpuBrand || 'CPU unavailable')}</small>
                </article>
                <article class="pc-stat-card">
                  <span>Memory</span>
                  <strong>${formatBytes(memoryUsed)} / ${formatBytes(memoryTotal)}</strong>
                  ${renderStatBar(stats.memoryUsedPercent, 'Memory usage')}
                  <small>${formatPercent(stats.memoryUsedPercent)} used</small>
                </article>
                <article class="pc-stat-card">
                  <span>Disk</span>
                  <strong>${formatBytes(diskUsed)} / ${formatBytes(stats.diskTotalBytes)}</strong>
                  ${renderStatBar(stats.diskUsedPercent, 'Disk usage')}
                  <small>${formatBytes(stats.diskAvailableBytes)} available</small>
                </article>
                <article class="pc-stat-card">
                  <span>Network total</span>
                  <strong>${formatBytes(networkTotal)}</strong>
                  <small>Down ${formatBytes(stats.networkReceivedBytes)} · Up ${formatBytes(stats.networkTransmittedBytes)}</small>
                </article>
                <article class="pc-stat-card">
                  <span>Uptime</span>
                  <strong>${escapeHtml(formatDuration(stats.uptimeSeconds))}</strong>
                  <small>System uptime reported by OS</small>
                </article>
                <article class="pc-stat-card">
                  <span>Temperature</span>
                  <strong>${stats.temperatureCelsius == null ? 'Unavailable' : `${Number(stats.temperatureCelsius).toFixed(0)}°C`}</strong>
                  <small>Highest reported sensor</small>
                </article>
              </div>
            </section>
            <section class="panel glass-panel pc-system-panel">
              <div class="panel-header">
                <span class="pill">Machine</span>
                <h3>System identity</h3>
                <p>High-level local machine metadata for diagnostics. This stays on-device.</p>
              </div>
              <div class="panel-body status-stack">
                <div><span>Host</span><strong>${escapeHtml(stats.hostName || 'Unavailable')}</strong></div>
                <div><span>OS</span><strong>${escapeHtml([stats.osName, stats.osVersion].filter(Boolean).join(' ') || 'Unavailable')}</strong></div>
                <div><span>Kernel</span><strong>${escapeHtml(stats.kernelVersion || 'Unavailable')}</strong></div>
                <div><span>CPU</span><strong>${escapeHtml(stats.cpuBrand || 'Unavailable')}</strong></div>
                <div><span>Last refresh</span><strong>${escapeHtml(collectedAt)}</strong></div>
              </div>
            </section>
            <section class="panel glass-panel pc-disk-panel">
              <div class="panel-header">
                <span class="pill">${disks.length} volumes</span>
                <h3>Disks</h3>
              </div>
              <div class="pc-disk-list">
                ${
                  disks.length === 0
                    ? '<article class="empty-state compact"><strong>No disk details available</strong><p>Run the installed desktop app and refresh this tab.</p></article>'
                    : disks
                        .slice(0, 8)
                        .map(
                          (disk) => `
                            <article>
                              <div>
                                <strong>${escapeHtml(disk.name || disk.mountPoint || 'Disk')}</strong>
                                <span>${escapeHtml(disk.mountPoint || 'Unknown mount point')}</span>
                              </div>
                              <div>
                                <strong>${formatPercent(disk.usedPercent)}</strong>
                                <span>${formatBytes(Number(disk.totalBytes || 0) - Number(disk.availableBytes || 0))} used of ${formatBytes(disk.totalBytes)}</span>
                              </div>
                              ${renderStatBar(disk.usedPercent, `${disk.name || 'Disk'} usage`)}
                            </article>
                          `,
                        )
                        .join('')
                }
              </div>
            </section>
          </div>
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
                <div><span>Model</span><strong>${escapeHtml(state.providerModel)}</strong></div>
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
  ['context', 'Context and thinking'],
  ['chat', 'Chat display'],
  ['telegram', 'Telegram'],
  ['security', 'Security'],
  ['advanced', 'Advanced'],
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
    workspace: `Current path: ${state.workspacePath}`,
    provider: `${provider.label}; auth ${state.providerAuthMethod}`,
    model: `${state.providerModel}; ${metadata.contextWindow}`,
    context: `${state.thinkingLevel} thinking; ${state.selectedContextAccess.length} context sources`,
    chat: state.showThinkingInChat ? 'Reasoning visible in chat' : 'Reasoning hidden in chat',
    telegram: state.selectedChannels.includes('telegram') ? 'Telegram selected' : 'Telegram off',
    security: labelFor(securityProfiles, state.securityProfile),
    advanced: `${labelFor(runtimeModes, state.runtimeMode)} runtime; fonts and diagnostics`,
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
        <small>Stored in workspace secrets, not YAML.</small>
      </label>
      ${renderSettingsOAuthPanel(state)}
    `;
  }

  if (activeSection === 'model') {
    return `
      <label>
        Model
        <select id="settings-provider-model">
          ${modelOptionsFor(provider, state.providerModel, state.providerAuthMethod)
            .map(
              (model) => `
                <option value="${escapeAttribute(model.id)}" ${selected(state.providerModel, model.id)}>${escapeHtml(model.label || model.id)}</option>
              `,
            )
            .join('')}
        </select>
      </label>
      <div class="settings-inline-note">
        <strong>${escapeHtml(metadata.contextWindow)}</strong>
        <p>${escapeHtml(metadata.detail)} ${escapeHtml(metadata.capabilities.join(', '))}.</p>
      </div>
      <label>
        API style
        <select id="provider-api">
          <option value="openai" ${selected(state.providerApi, 'openai')}>OpenAI-compatible</option>
          <option value="anthropic" ${selected(state.providerApi, 'anthropic')}>Anthropic-compatible</option>
        </select>
      </label>
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
        <textarea id="profile-purpose" placeholder="How should Argentum behave in this workspace?">${escapeHtml(state.systemPrompt)}</textarea>
      </label>
    `;
  }

  return `
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
    <div class="runtime-mode-tabs full-span" aria-label="Runtime behavior">
      ${runtimeModes
        .map(
          (mode) => `
            <button class="runtime-mode-pill ${state.runtimeMode === mode.id ? 'active' : ''}" data-runtime-mode="${escapeAttribute(mode.id)}">
              <strong>${escapeHtml(mode.label)}</strong>
              <span>${escapeHtml(mode.headline)}</span>
            </button>
          `,
        )
        .join('')}
    </div>
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
      [
        'Telegram',
        state.desktopState?.telegramDiagnostics?.lastResponseStatus ||
          (state.selectedChannels.includes('telegram') ? 'Configured, waiting for bot status' : 'Not selected'),
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
          ${renderProductHero({
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

export const modules = {
  onboarding: onboardingModule,
  chat: chatModule,
  gateway: gatewayModule(),
  logs: logsModule,
  security: securityModule,
  'pc-stats': pcStatsModule,
  settings: settingsModule,
  diagnostics: diagnosticsModule,
};
