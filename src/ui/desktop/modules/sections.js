import { channelOptions, providerPresets, runtimeModes, securityProfiles } from './constants.js';
import { chatModule } from './chat.js';
import { onboardingModule } from './onboarding.js';
import {
  formatFound,
  formatWorkspaceHealth,
  renderActionCards,
  renderHero,
  renderNotifications,
} from './shell.js';
import { currentProvider, escapeHtml, labelFor } from './utils.js';

function honestModule(id, label, heading, detail) {
  return {
    id,
    label,
    validate: () => '',
    healthCheck: (state) => ({
      status: state.setupComplete ? 'pending' : 'blocked',
      message: state.setupComplete ? `${label} is staged for runtime wiring.` : 'Finish onboarding first.',
    }),
    render: (state) => `
      ${renderNotifications()}
      ${renderHero(label, heading, detail, [
        { label: 'Status', value: state.setupComplete ? 'Planned' : 'Setup first' },
        { label: 'Isolation', value: 'Module-safe' },
        { label: 'Repair', value: 'Approval-gated' },
      ])}
      <section class="panel honest-state">
        <div class="panel-body">
          <span class="pill warn">Not fully wired yet</span>
          <h3>This module will not pretend to be ready.</h3>
          <p>Its UI is isolated from Chat, Security, Settings, and Diagnostics. If this surface fails later, Argentum should keep the rest of the app usable and suggest repair actions only with your approval.</p>
        </div>
      </section>
      ${renderActionCards(id)}
    `,
  };
}

const agentsModule = honestModule(
  'agents',
  'Agents',
  'Create focused agents with scoped permissions',
  'Profiles will separate research, coding, operations, and personal workflows without sharing unlimited capabilities.',
);

const runnerModule = honestModule(
  'runner',
  'Agent Runner',
  'Run work through visible steps and approvals',
  'Agent runs should show every privileged step before execution and keep repairs inside approved folders.',
);

const skillsModule = honestModule(
  'skills',
  'Skills Library',
  'Install only the capabilities you mean to trust',
  'Skills will be discoverable from the GUI while mapping back to terminal commands and permission grants.',
);

const graphModule = {
  id: 'graph',
  label: 'Knowledge Graph',
  validate: () => '',
  healthCheck: () => ({ status: 'pending', message: 'Graph visualization is local-only preview.' }),
  render: (state) => `
    ${renderNotifications()}
    ${renderHero(
      'Knowledge graph',
      'A visual map for memory and decisions',
      'This is a contained preview surface. It should become more human and less generic as the workspace canvas matures.',
      [
        { label: 'Nodes', value: 'Preview' },
        { label: 'Boundary', value: 'Workspace' },
        { label: 'Mode', value: 'Read-only' },
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
  `,
};

const memoryModule = honestModule(
  'memory',
  'Memory',
  'Search and inspect local recall before using it',
  'Memory stays under the selected workspace and should be inspectable before agents use it.',
);

const logsModule = {
  id: 'logs',
  label: 'Activity Logs',
  validate: () => '',
  healthCheck: (state) => ({
    status: state.desktopState.logsExists ? 'ok' : 'pending',
    message: state.desktopState.logsExists ? 'Logs directory exists.' : 'Logs appear after setup or runtime activity.',
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
  `,
};

const webchatModule = {
  id: 'webchat',
  label: 'Channels',
  validate: () => '',
  healthCheck: (state) => ({
    status: state.selectedChannels.length > 0 ? 'ok' : 'degraded',
    message: `Selected channels: ${state.selectedChannels.join(', ')}`,
  }),
  render: (state) => `
    ${renderNotifications()}
    ${renderHero(
      'Channels',
      'Expose Argentum only where you choose',
      'Local app access is always on. Webchat, Telegram, and WhatsApp are opt-in and remain isolated if they are unhealthy.',
      [
        { label: 'Enabled', value: state.selectedChannels.map((id) => labelFor(channelOptions, id)).join(', ') },
        { label: 'Local', value: 'Always on' },
        { label: 'External', value: state.selectedChannels.length > 1 ? 'Opt-in' : 'Off' },
      ],
    )}
    <div class="card-grid">
      ${channelOptions
        .map(
          (channel) => `
            <article class="feature-card">
              <span class="pill ${state.selectedChannels.includes(channel.id) ? 'ok' : ''}">${state.selectedChannels.includes(channel.id) ? 'Selected' : 'Off'}</span>
              <h3>${escapeHtml(channel.label)}</h3>
              <p>${escapeHtml(channel.detail)}</p>
            </article>
          `,
        )
        .join('')}
    </div>
    ${renderActionCards('webchat')}
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
                <button class="button" data-approval="${approval.id}">Review</button>
              </div>
            `,
          )
          .join('')}
      </div>
    </section>
    ${renderActionCards('security')}
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
        'Desktop controls save to the same workspace config that the CLI reads.',
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
      </section>
      ${renderActionCards('settings')}
    `;
  },
};

const diagnosticsModule = {
  id: 'diagnostics',
  label: 'Diagnostics',
  validate: () => '',
  healthCheck: (state) => ({
    status: state.desktopState.configExists ? 'ok' : 'pending',
    message: state.desktopState.configExists ? 'Configuration exists.' : 'Configuration has not been saved yet.',
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
        'Checks are modular. A failed provider, channel, or log check should not stop Chat or Security from opening.',
        [
          { label: 'Config', value: state.setupComplete ? 'Saved' : 'Pending' },
          { label: 'Repair', value: 'Approval-gated' },
          { label: 'Modules', value: 'Isolated' },
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
                  ${ok ? '' : '<button class="button" data-repair-action="review">Review</button>'}
                </div>
              `,
            )
            .join('')}
        </div>
      </section>
      ${renderActionCards('diagnostics')}
    `;
  },
};

export const modules = {
  onboarding: onboardingModule,
  chat: chatModule,
  agents: agentsModule,
  runner: runnerModule,
  skills: skillsModule,
  webchat: webchatModule,
  graph: graphModule,
  memory: memoryModule,
  logs: logsModule,
  security: securityModule,
  settings: settingsModule,
  diagnostics: diagnosticsModule,
};
