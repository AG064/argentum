import { providerPresets, securityProfiles } from './constants.js';
import { renderActionCards, renderHero, renderNotifications, renderStatusRail } from './shell.js';
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
      'Local guided chat',
      'Start with Argentum, then connect live models',
      'The first chat runs locally so setup can finish even when a provider is offline. Live model calls are enabled after the provider test passes.',
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
            <p>Argentum can place choices, warnings, summaries, and action cards directly in chat.</p>
          </div>
          <span class="pill ${state.apiTest.status === 'ok' ? 'ok' : 'warn'}">${state.apiTest.status === 'ok' ? 'Provider ready' : 'Offline mode'}</span>
        </div>
        <div class="chat-transcript">
          ${state.chatBlocks.map((block) => renderChatBlock(block)).join('')}
        </div>
        <div class="composer">
          <textarea id="chat-draft" placeholder="Tell Argentum what to call you, or ask what to configure next.">${escapeHtml(state.draftMessage)}</textarea>
          <button class="button primary" id="send-chat">Send</button>
        </div>
      </section>
      ${renderStatusRail()}
    </div>
    ${renderActionCards('chat')}
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
    <article class="message ${block.role === 'user' ? 'system' : 'assistant'}">
      <span>${escapeHtml(block.title || 'Argentum')}</span>
      <p>${escapeHtml(block.body)}</p>
    </article>
  `;
}
