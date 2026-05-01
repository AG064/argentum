import { commandCatalog, providerPresets, securityProfiles, sections } from './constants.js';
import { icon } from './icons.js';
import { state } from './state.js';
import { currentProvider, escapeAttribute, escapeHtml, labelFor } from './utils.js';

export function renderNavigation() {
  return sections
    .filter((section) => state.setupComplete || section.id !== 'onboarding')
    .map(
      (section) => `
        <button class="nav-button ${section.id === state.activeSection ? 'active' : ''}" data-section="${section.id}">
          <span class="nav-icon">${icon(section.icon)}</span>
          <span>${escapeHtml(section.title)}</span>
        </button>
      `,
    )
    .join('');
}

export function renderNotifications() {
  if (state.notifications.length === 0) return '';

  return `
    <div class="notification-stack" aria-live="polite">
      ${state.notifications
        .map(
          (notification) => `
            <article class="notification ${escapeAttribute(notification.type)}">
              <div>
                <strong>${escapeHtml(notification.title)}</strong>
                <p>${escapeHtml(notification.message)}</p>
              </div>
              <button class="icon-button compact" data-dismiss-notification="${escapeAttribute(notification.id)}" aria-label="Dismiss notification">x</button>
            </article>
          `,
        )
        .join('')}
    </div>
  `;
}

export function renderHero(kicker, heading, detail, stats = []) {
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

export function renderActionCards(sectionId, options = {}) {
  const actions = commandCatalog.filter((action) => action.section === sectionId);
  const items = options.limit ? actions.slice(0, options.limit) : actions;

  if (items.length === 0) {
    return `
      <section class="panel honest-state">
        <div class="panel-body">
          <span class="pill warn">Not wired yet</span>
          <h3>This surface is waiting for its runtime commands.</h3>
          <p>Argentum will keep this module isolated so unfinished work here does not block Chat, Security, Settings, or Diagnostics.</p>
        </div>
      </section>
    `;
  }

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
                <button class="button primary" data-run-action="${escapeAttribute(action.id)}">Prepare</button>
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
        <h3>Session State</h3>
      </div>
      <div class="panel-body status-stack">
        <div>
          <span>Workspace</span>
          <strong>${escapeHtml(state.workspacePath)}</strong>
        </div>
        <div>
          <span>Provider</span>
          <strong>${escapeHtml(provider.label)}</strong>
        </div>
        <div>
          <span>Permission profile</span>
          <strong>${escapeHtml(labelFor(securityProfiles, state.securityProfile))}</strong>
        </div>
        <div>
          <span>Last action</span>
          <strong>${escapeHtml(state.actionStatus)}</strong>
        </div>
      </div>
    </aside>
  `;
}

export function renderModule(module) {
  try {
    return module.render(state);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return `
      <section class="panel module-error">
        <div class="panel-body">
          <span class="pill warn">Module contained</span>
          <h2>${escapeHtml(module.label || module.id)} could not render.</h2>
          <p>${escapeHtml(message)}</p>
          <p>Other Argentum modules remain available from the navigation rail.</p>
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
