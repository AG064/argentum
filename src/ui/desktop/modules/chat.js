import { renderNotifications } from './shell.js';
import { escapeAttribute, escapeHtml, selected } from './utils.js';

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
  return `
    ${renderNotifications()}
    <div class="chat-workspace">
      <aside class="panel recent-chat-list">
        <div class="panel-header">
          <h3>Recent Chats</h3>
        </div>
        <div class="recent-chat-items">
          ${state.recentChats
            .map(
              (chat) => `
                <button class="recent-chat-item" data-recent-chat="${escapeAttribute(chat.id)}">
                  <strong>${escapeHtml(chat.title)}</strong>
                  <span>${escapeHtml(chat.subtitle)}</span>
                </button>
              `,
            )
            .join('')}
        </div>
      </aside>
      <section class="panel chat-shell">
        <div class="panel-header split-header">
          <div>
            <h3>Conversation</h3>
            <p>${escapeHtml(state.apiTest.status === 'ok' ? 'Live model replies are active.' : 'Offline guided mode is active until provider testing passes.')}</p>
          </div>
          <span class="pill ${state.apiTest.status === 'ok' ? 'ok' : 'warn'}">${state.apiTest.status === 'ok' ? 'Provider ready' : 'Offline mode'}</span>
        </div>
        <div class="chat-transcript">
          ${state.chatBlocks.map((block) => renderChatBlock(block)).join('')}
        </div>
        <div class="composer">
          <div class="composer-tools">
            <button class="icon-button" id="attach-file" title="Attach file" aria-label="Attach file"><span data-icon="paperclip"></span></button>
            <button class="icon-button" id="voice-input" title="Use microphone" aria-label="Use microphone">${state.voiceInputStatus === 'listening' ? '...' : '<span data-icon="mic"></span>'}</button>
            <label>
              Thinking
              <select id="thinking-level">
                <option value="fast" ${selected(state.thinkingLevel, 'fast')}>Fast</option>
                <option value="balanced" ${selected(state.thinkingLevel, 'balanced')}>Balanced</option>
                <option value="deep" ${selected(state.thinkingLevel, 'deep')}>Deep</option>
              </select>
            </label>
          </div>
          ${renderAttachmentTray(state)}
          <textarea id="chat-draft" placeholder="Tell Argentum what to call you, or ask what to configure next.">${escapeHtml(state.draftMessage)}</textarea>
          <button class="button primary" id="send-chat">Send</button>
        </div>
      </section>
    </div>
  `;
}

function renderAttachmentTray(state) {
  if (state.chatAttachments.length === 0) return '';

  return `
    <div class="attachment-tray">
      ${state.chatAttachments
        .map(
          (file) => `
            <span class="attachment-chip">${escapeHtml(file.name)}</span>
          `,
        )
        .join('')}
    </div>
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
