import { providerPresets, securityProfiles } from './constants.js';
import { renderHero, renderNotifications, renderStatusRail } from './shell.js';
import { currentProvider, escapeAttribute, escapeHtml, labelFor } from './utils.js';

export const chatModule = {
  id: 'chat',
  label: 'Chat',
  render: renderChatSection,
  validate: () => '',
  healthCheck: (state) => ({
    status: state.apiTest.status === 'error' ? 'degraded' : 'ok',
    message:
      state.apiTest.status === 'error'
        ? 'Chat is running in offline guided mode until provider settings are fixed.'
        : 'Chat is ready.',
  }),
};

function renderChatSection(state) {
  const provider = currentProvider(providerPresets, state);
  return `
    ${renderNotifications()}
    ${renderHero(
      'Chat',
      'Work locally first, connect live models when ready',
      'The app can answer setup and status questions locally. Live model calls activate only after provider settings pass.',
      [
        { label: 'Provider', value: provider.label },
        { label: 'Mode', value: state.apiTest.status === 'ok' ? 'Live-ready' : 'Offline guided' },
        { label: 'Security', value: labelFor(securityProfiles, state.securityProfile) },
      ],
    )}
    <div class="workspace-layout">
      <section class="panel chat-shell">
        <div class="panel-header split-header">
          <div>
            <h3>Conversation</h3>
            <p>Live provider replies are used after provider testing passes. Otherwise Argentum stays in clear offline setup mode.</p>
          </div>
          <span class="pill ${state.apiTest.status === 'ok' ? 'ok' : 'warn'}">${state.apiTest.status === 'ok' ? 'Provider ready' : 'Offline mode'}</span>
        </div>
        <div class="chat-action-row">
          <button class="button" data-chat-action="test-provider">Test Provider</button>
          <button class="button" data-chat-action="gateway-start">Start Gateway</button>
          <button class="button" data-chat-action="gateway-status">Check Gateway</button>
          <button class="button" data-chat-action="settings">Open Settings</button>
        </div>
        <div class="chat-transcript">
          ${state.chatBlocks.map((block) => renderChatBlock(block)).join('')}
        </div>
        <div class="composer">
          <textarea id="chat-draft" placeholder="Tell Argentum what to call you, or ask what to configure next.">${escapeHtml(state.draftMessage)}</textarea>
          <button class="button primary" id="send-chat">Send</button>
        </div>
      </section>
      <aside class="side-stack">
        ${renderProfilePanel(state)}
        ${renderStatusRail()}
        ${renderTerminalPanel(state)}
      </aside>
    </div>
  `;
}

function renderProfilePanel(state) {
  return `
    <section class="panel profile-panel">
      <div class="panel-header">
        <h3>Profile</h3>
        <p>Replace the old local setup loop with clear fields you can change anytime.</p>
      </div>
      <div class="panel-body form-grid">
        <label>
          Your name
          <input id="profile-user-name" value="${escapeAttribute(state.userName)}" placeholder="Example: AG" />
        </label>
        <label>
          Agent name
          <input id="profile-agent-name" value="${escapeAttribute(state.agentName)}" placeholder="Argentum" />
        </label>
        <label>
          Main purpose
          <textarea id="profile-purpose" placeholder="What should this workspace help with?">${escapeHtml(state.agentPurpose)}</textarea>
        </label>
        <button class="button primary" id="save-profile">Save profile</button>
      </div>
    </section>
  `;
}

function renderTerminalPanel(state) {
  return `
    <section class="panel terminal-panel">
      <div class="panel-header split-header">
        <h3>Terminal</h3>
        <span class="pill">Preview</span>
      </div>
      <div class="terminal-body">
        ${state.terminalEntries
          .map(
            (entry) => `
              <article class="terminal-entry ${escapeAttribute(entry.status)}">
                <code>$ ${escapeHtml(entry.command)}</code>
                <pre>${escapeHtml(entry.output)}</pre>
              </article>
            `,
          )
          .join('')}
      </div>
    </section>
  `;
}

function renderChatBlock(block) {
  if (block.type === 'optionGroup') {
    return `
      <article class="chat-block option-group">
        <span>${escapeHtml(block.title)}</span>
        <p>${escapeHtml(block.body)}</p>
        <div class="option-grid">
          ${block.options
            .map(
              (option) => `
                <button class="option-card" data-chat-option="${escapeAttribute(option.id)}">
                  <strong>${escapeHtml(option.label)}</strong>
                  <small>${escapeHtml(option.detail)}</small>
                </button>
              `,
            )
            .join('')}
        </div>
      </article>
    `;
  }

  if (block.type === 'warning' || block.type === 'summary' || block.type === 'actionCard') {
    return `
      <article class="chat-block ${escapeAttribute(block.type)}">
        <span>${escapeHtml(block.title)}</span>
        <p>${escapeHtml(block.body)}</p>
      </article>
    `;
  }

  return `
    <article class="message ${block.role === 'user' ? 'user' : 'assistant'}">
      <span>${escapeHtml(block.title || 'Argentum')}</span>
      <p>${escapeHtml(block.body)}</p>
    </article>
  `;
}
