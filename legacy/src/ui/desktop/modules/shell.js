import { commandCatalog, providerPresets, securityProfiles, sections } from './constants.js';
import { icon } from './icons.js';
import { state } from './state.js';
import {
  currentProvider,
  displayModelName,
  escapeAttribute,
  escapeHtml,
  labelFor,
} from './utils.js';

const navigationGroups = [
  {
    label: 'Agent',
    sectionIds: ['chat'],
  },
  {
    label: 'Run',
    sectionIds: ['gateway', 'local-server'],
  },
  {
    label: 'Control',
    sectionIds: ['security', 'pc-stats'],
  },
  {
    label: 'Manage',
    sectionIds: ['settings', 'diagnostics', 'logs', 'update'],
  },
];

const navigationLabels = {
  'chat': 'Tasks',
  'gateway': 'Gateway',
  'local-server': 'Local models',
  'security': 'Permissions',
  'pc-stats': 'System',
  'settings': 'Settings',
  'diagnostics': 'Diagnostics',
  'logs': 'Activity',
  'update': 'Updates',
};

export function renderNavigation() {
  const byId = new Map(
    sections
      .filter((section) => section.id !== 'onboarding')
      .map((section) => [section.id, section]),
  );

  return navigationGroups
    .map((group) => {
      const items = group.sectionIds
        .map((sectionId) => byId.get(sectionId))
        .filter(Boolean)
        .map((section) => {
          const active = section.id === state.activeSection;
          const hasUpdate = section.id === 'update' && state.updateAvailable;
          return `
            <button
              class="nav-item ${active ? 'is-active' : ''} ${hasUpdate ? 'has-update' : ''}"
              type="button"
              data-section="${escapeAttribute(section.id)}"
              aria-current="${active ? 'page' : 'false'}"
              title="${escapeAttribute(section.title)}"
            >
              <span class="nav-item-icon" aria-hidden="true">${icon(section.icon)}</span>
              <span class="nav-item-label">${escapeHtml(navigationLabels[section.id] || section.title)}</span>
              ${hasUpdate ? '<span class="nav-update-dot" aria-label="Update available"></span>' : ''}
            </button>
          `;
        })
        .join('');

      return `
        <section class="nav-group" aria-label="${escapeAttribute(group.label)}">
          <h2>${escapeHtml(group.label)}</h2>
          <div class="nav-group-items">${items}</div>
        </section>
      `;
    })
    .join('');
}

export function renderNotifications() {
  if (state.notifications.length === 0) return '';

  return `
    <div class="notification-layer" aria-live="polite">
      <div class="notification-stack">
        ${state.notifications
          .map(
            (notification) => `
              <article class="notification-toast ${escapeAttribute(notification.type)}">
                <span class="notification-indicator" aria-hidden="true"></span>
                <div>
                  <strong>${escapeHtml(notification.title)}</strong>
                  <p>${escapeHtml(notification.message)}</p>
                </div>
                <button
                  class="icon-button compact"
                  type="button"
                  data-dismiss-notification="${escapeAttribute(notification.id)}"
                  aria-label="Dismiss notification"
                ><span data-icon="x" aria-hidden="true"></span></button>
              </article>
            `,
          )
          .join('')}
      </div>
    </div>
  `;
}

export function renderProviderStatusPill() {
  const provider = currentProvider(providerPresets, state);
  const ready = state.apiTest.status === 'ok';
  const testing = state.apiTest.status === 'testing';
  const status = testing ? 'Checking' : ready ? 'Ready' : 'Setup needed';
  const tone = testing ? 'is-busy' : ready ? 'is-ready' : 'needs-attention';
  const model = displayModelName(state.providerModel);

  return `
    <button
      class="provider-pill ${tone}"
      type="button"
      data-section="settings"
      title="${escapeAttribute(`${provider.label}, ${model}. ${state.apiTest.message || status}`)}"
    >
      <span class="provider-state-dot" aria-hidden="true"></span>
      <span class="provider-pill-copy">
        <strong>${escapeHtml(provider.label)}</strong>
        <small>${escapeHtml(status)}</small>
      </span>
    </button>
  `;
}

export function renderHero(kicker, heading, detail, stats = []) {
  return `
    <header class="page-intro">
      <div>
        <p class="section-kicker">${escapeHtml(kicker)}</p>
        <h2>${escapeHtml(heading)}</h2>
        <p>${escapeHtml(detail)}</p>
      </div>
      ${
        stats.length
          ? `<dl class="summary-list">
              ${stats
                .map(
                  (stat) => `
                    <div>
                      <dt>${escapeHtml(stat.label)}</dt>
                      <dd>${escapeHtml(stat.value)}</dd>
                    </div>
                  `,
                )
                .join('')}
            </dl>`
          : ''
      }
    </header>
  `;
}

export function renderActionCards(sectionId, options = {}) {
  const actions = commandCatalog.filter((action) => action.section === sectionId);
  const items = options.limit ? actions.slice(0, options.limit) : actions;
  if (items.length === 0) return '';

  return `
    <div class="command-list">
      ${items
        .map(
          (action) => `
            <article class="command-item">
              <div class="command-item-copy">
                <span class="status-tag">${escapeHtml(action.risk)}</span>
                <h3>${escapeHtml(action.title)}</h3>
                <p>${escapeHtml(action.summary)}</p>
                <code>${escapeHtml(action.command)}</code>
              </div>
              <div class="command-item-actions">
                <button class="button quiet" type="button" data-copy-command="${escapeAttribute(action.command)}">Copy</button>
                <button
                  class="button primary"
                  type="button"
                  data-run-action="${escapeAttribute(action.id)}"
                  ${state.runningAction === action.id ? 'disabled' : ''}
                >${state.runningAction === action.id ? 'Running...' : escapeHtml(action.buttonLabel || 'Run')}</button>
              </div>
            </article>
          `,
        )
        .join('')}
    </div>
  `;
}

export function renderStatusRail() {
  const provider = currentProvider(providerPresets, state);
  return `
    <aside class="panel status-panel">
      <div class="panel-header">
        <div>
          <span class="section-kicker">Current session</span>
          <h3>Status</h3>
        </div>
      </div>
      <dl class="definition-list">
        <div>
          <dt>Workspace</dt>
          <dd>${escapeHtml(state.workspacePath || 'Not selected')}</dd>
        </div>
        <div>
          <dt>Provider</dt>
          <dd>${escapeHtml(provider.label)}</dd>
        </div>
        <div>
          <dt>Permissions</dt>
          <dd>${escapeHtml(labelFor(securityProfiles, state.securityProfile))}</dd>
        </div>
        <div>
          <dt>Last action</dt>
          <dd>${escapeHtml(state.actionStatus)}</dd>
        </div>
      </dl>
    </aside>
  `;
}

export function renderModule(module) {
  try {
    return module.render(state);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return `
      <section class="module-error" role="alert">
        <span class="status-tag danger">Module error</span>
        <h2>${escapeHtml(module.label || module.id)} could not open</h2>
        <p>${escapeHtml(message)}</p>
        <p>Argentum is still running. Use another section or report this error from Help.</p>
        <div class="button-row">
          <button class="button primary" type="button" data-section="diagnostics">Open diagnostics</button>
          <button class="button" type="button" data-feedback-url="https://github.com/AG064/argentum/issues/new?labels=bug">Report issue</button>
        </div>
      </section>
    `;
  }
}

export function formatWorkspaceHealth() {
  if (!state.desktopState?.workspaceReady) return 'Not created';
  if (state.desktopState.configExists && state.desktopState.dataExists) return 'Ready';
  return 'Needs setup';
}

export function formatFound(value) {
  return value ? 'Found' : 'Missing';
}
