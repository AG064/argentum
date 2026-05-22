import { modelMetadata, providerPresets, thinkingLevels } from './constants.js';
import { renderNotifications } from './shell.js';
import {
  currentProvider,
  contextTokenLimit,
  contextUsagePercent,
  escapeAttribute,
  escapeHtml,
  estimateRuntimeContextTokens,
  filePreviewUrl,
  displayModelName,
  modelMetadataFor,
  renderMarkdown,
  selected,
} from './utils.js';

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
  const metadata = modelMetadataFor(state.providerModel, modelMetadata);
  const localContextTokens = estimateRuntimeContextTokens(state, state.draftMessage);
  const providerContextTokens = numericUsageValue(state.usageSnapshot?.contextTokens);
  const providerContextLimit = numericUsageValue(state.usageSnapshot?.contextTokenLimit);
  const effectiveMetadata =
    providerContextLimit > 0
      ? {
          ...metadata,
          maxContextTokens: providerContextLimit,
          contextWindow: `${providerContextLimit.toLocaleString()} tokens reported`,
          currentContextLabel: state.usageSnapshot?.contextSource || metadata.currentContextLabel,
        }
      : metadata;
  const estimatedTokens =
    Number.isFinite(providerContextTokens) && providerContextTokens > 0
      ? Math.max(localContextTokens, providerContextTokens)
      : localContextTokens;
  const contextPercent = contextUsagePercent(estimatedTokens, effectiveMetadata);
  const providerReady = state.apiTest.status === 'ok';
  const modelDisplay = displayModelName(state.providerModel);
  const shellClasses = [
    'chat-product-shell',
    'arg-chat-shell',
    `view-mode-${escapeAttribute(state.viewMode || 'chat')}`,
    state.conversationsCollapsed ? 'conversation-collapsed' : '',
    state.inspectorCollapsed ? 'inspector-collapsed' : '',
  ]
    .filter(Boolean)
    .join(' ');

  return `
    ${renderNotifications()}
    <div class="${shellClasses}">
      <aside class="conversation-column arg-conversation-rail panel recent-chat-list">
        <div class="conversation-column-header">
          <div class="split-header">
            <h3>Conversations</h3>
            <div class="button-row tight">
              <button class="icon-button" data-conversation-settings="true" title="Conversation settings" aria-label="Conversation settings"><span data-icon="list"></span></button>
              <button class="icon-button" data-toggle-chat-panel="conversations" title="Hide conversations" aria-label="Hide conversations"><span data-icon="chevrons-left"></span></button>
            </div>
          </div>
          <label class="search-box">
            <span data-icon="search"></span>
            <input id="conversation-search" placeholder="Search conversations" />
            <kbd>Ctrl K</kbd>
          </label>
          <button class="button primary wide button-icon-label" id="new-chat"><span data-icon="plus"></span><span>New chat</span></button>
          <div class="conversation-tabs" role="tablist" aria-label="Conversation filters">
            <button class="${state.chatFilter === 'all' ? 'active' : ''}" data-chat-filter="all" type="button">All</button>
            <button class="${state.chatFilter === 'pinned' ? 'active' : ''}" data-chat-filter="pinned" type="button">Pinned</button>
          </div>
        </div>
        <div class="recent-chat-items conversation-history">
          ${renderConversationList(state)}
        </div>
      </aside>
      <section class="chat-canvas arg-chat-stage panel chat-shell">
        <div class="chat-canvas-header arg-chat-toolbar split-header">
          <div>
            <span class="eyebrow">Workspace</span>
            <h3>${escapeHtml(state.chatSessions.find((chat) => chat.id === state.activeChatId)?.title || 'Chat')}</h3>
            <p>${escapeHtml(providerReady ? 'Live model replies are active.' : 'Offline guided mode is active until provider testing passes.')}</p>
          </div>
          <div class="model-context-summary">
            <span class="pill ${providerReady ? 'ok' : 'warn'}">${providerReady ? 'Provider ready' : 'Offline mode'}</span>
            <strong>${escapeHtml(modelDisplay)}</strong>
            <small>${escapeHtml(provider.label)}</small>
            <small>${escapeHtml(metadata.currentContextLabel)}</small>
          </div>
        </div>
        <div class="chat-transcript arg-message-canvas" aria-live="polite">
          ${renderWorkspaceEmptyState(state, provider, metadata)}
          ${renderChatDayDivider(state)}
          ${state.chatBlocks.map((block) => renderMessageComponent(block, state)).join('')}
        </div>
        ${renderNewTransmissionBar(state)}
        ${renderTypingIndicator(state)}
        ${renderComposer(state, effectiveMetadata, estimatedTokens, contextPercent)}
      </section>
      ${renderInspector(state, provider)}
      ${renderChatTelemetry(state, provider, effectiveMetadata, estimatedTokens, contextPercent)}
      <button class="chat-panel-restore chat-panel-restore-left" data-toggle-chat-panel="conversations" type="button" title="Show conversations" aria-label="Show conversations">
        <span data-icon="panelLeft"></span>
        <span>Chats</span>
      </button>
      <button class="chat-panel-restore chat-panel-restore-right" data-toggle-chat-panel="inspector" type="button" title="Show inspector" aria-label="Show inspector">
        <span>Inspector</span>
        <span data-icon="panelRight"></span>
      </button>
    </div>
  `;
}

function modelSupportsThinking(metadata) {
  return (metadata.capabilities || []).some((capability) => {
    const value = String(capability).toLowerCase();
    if (value.includes('when model supports')) return false;
    return value.includes('reasoning') || value.includes('thinking');
  });
}

function renderConversationList(state) {
  let lastGroup = '';
  const sessions = filteredChatSessions(state);
  if (sessions.length === 0) {
    return `
      <div class="conversation-empty-state">
        <span data-icon="${state.chatFilter === 'pinned' ? 'pin' : 'chat'}"></span>
        <strong>${state.chatFilter === 'pinned' ? 'No pinned chats' : 'No conversations'}</strong>
        <p>${state.chatFilter === 'pinned' ? 'Pin a chat from its menu to keep it here.' : 'Start a new chat to create a session.'}</p>
      </div>
    `;
  }

  return sessions
    .map((chat) => {
      const group = conversationGroup(chat.lastMessageAt || chat.updatedAt);
      const groupMarkup =
        group !== lastGroup
          ? `<div class="conversation-group-label">${escapeHtml(group)}</div>`
          : '';
      lastGroup = group;

      return `
        ${groupMarkup}
        <article class="recent-chat-item arg-conversation-card ${state.activeChatId === chat.id ? 'active' : ''} ${chat.unreadCount ? 'unread' : ''} ${state.conversationMenuChatId === chat.id ? 'menu-open' : ''}" data-conversation-card="${escapeAttribute(chat.id)}">
          <button class="recent-chat-main" data-recent-chat="${escapeAttribute(chat.id)}">
            <strong>${chat.pinned ? '<span data-icon="pin"></span>' : ''}${escapeHtml(chat.title)}</strong>
            <span>${escapeHtml(chat.subtitle)}</span>
          </button>
          <time>${escapeHtml(formatChatTime(chat.lastMessageAt || chat.updatedAt))}</time>
          ${chat.unreadCount ? `<span class="unread-dot" aria-label="New messages"></span>` : ''}
          <button class="icon-button compact recent-chat-delete danger-x" data-delete-chat="${escapeAttribute(chat.id)}" title="Delete chat" aria-label="Delete chat"><span data-icon="x"></span></button>
          <button class="icon-button compact recent-chat-more" data-conversation-menu="${escapeAttribute(chat.id)}" title="Conversation actions" aria-label="Conversation actions"><span data-icon="moreVertical"></span></button>
          ${renderConversationMenu(chat, state)}
          ${
            state.pendingDeleteChatId === chat.id
              ? `
                <div class="chat-delete-confirm">
                  <span>Delete this chat?</span>
                  <button class="button danger" data-confirm-delete-chat="${escapeAttribute(chat.id)}">Delete</button>
                  <button class="button" data-cancel-delete-chat="true">Cancel</button>
                </div>
              `
              : ''
          }
        </article>
      `;
    })
    .join('');
}

function filteredChatSessions(state) {
  const sessions = Array.isArray(state.chatSessions) ? state.chatSessions : [];
  if (state.chatFilter === 'pinned') return sessions.filter((chat) => chat.pinned);
  return sessions;
}

function renderConversationMenu(chat, state) {
  if (state.conversationMenuChatId !== chat.id) return '';

  return `
    <div class="conversation-action-menu" role="menu">
      <button type="button" data-pin-chat="${escapeAttribute(chat.id)}">
        <span data-icon="pin"></span>
        ${chat.pinned ? 'Unpin conversation' : 'Pin conversation'}
      </button>
      <button type="button" data-rename-chat="${escapeAttribute(chat.id)}">
        <span data-icon="edit"></span>
        Rename
      </button>
      <button type="button" data-clear-chat="${escapeAttribute(chat.id)}">
        <span data-icon="refresh"></span>
        Clear messages
      </button>
      <button type="button" class="disabled" title="Export is planned for a later build" disabled>
        <span data-icon="download"></span>
        Export
      </button>
      <button type="button" class="danger-text" data-delete-chat="${escapeAttribute(chat.id)}">
        <span data-icon="trash"></span>
        Delete
      </button>
    </div>
  `;
}

function conversationGroup(timestamp) {
  const now = new Date();
  const date = new Date(timestamp || Date.now());
  const diff = now.setHours(0, 0, 0, 0) - new Date(date).setHours(0, 0, 0, 0);
  if (diff <= 0) return 'Today';
  if (diff <= 86400000) return 'Yesterday';
  if (diff <= 604800000) return 'This week';
  return 'Older';
}

function formatChatTime(timestamp) {
  return new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  }).format(new Date(timestamp || Date.now()));
}

function renderWorkspaceEmptyState(state, provider, metadata) {
  const hasUserMessage = state.chatBlocks.some((block) => block.role === 'user');
  if (hasUserMessage) return '';

  return `
    <section class="workspace-empty-state fresh-conversation-hero fresh-action-grid">
      <div class="fresh-state-mark" aria-hidden="true">
        <span></span>
      </div>
      <span class="pill ok">Fresh conversation</span>
      <h2>Welcome to Argentum</h2>
      <p>Start with a prompt, inspect context, or open a runtime tool. Argentum stays inside your approved workspace and permissions.</p>
      ${renderPromptSuggestions()}
      <div class="workspace-action-grid">
        <button class="workspace-action-card" data-section="security">
          <span data-icon="shield"></span>
          <strong>Inspect context</strong>
          <small>Review what Argentum may see.</small>
        </button>
        <button class="workspace-action-card" data-section="gateway">
          <span data-icon="terminal"></span>
          <strong>Gateway</strong>
          <small>Start or check local runtime.</small>
        </button>
        <button class="workspace-action-card" data-section="settings">
          <span data-icon="settings"></span>
          <strong>Provider</strong>
          <small>${escapeHtml(provider.label)} · ${escapeHtml(metadata.contextWindow)}</small>
        </button>
        <button class="workspace-action-card" data-chat-option="security-policy">
          <span data-icon="lock"></span>
          <strong>Security</strong>
          <small>Check approval policy.</small>
        </button>
      </div>
      <article class="fresh-helper-card">
        <strong>${escapeHtml(state.agentName || 'Argentum')}</strong>
        <p>I can answer normally, inspect approved app context, and help run fixed desktop actions when you ask.</p>
      </article>
    </section>
  `;
}

function numericUsageValue(value) {
  const normalized = String(value || '').replace(/[^\d.]/g, '');
  const parsed = Number(normalized);
  return Number.isFinite(parsed) ? parsed : 0;
}

function renderChatDayDivider(state) {
  const hasMessages = state.chatBlocks.some((block) => block.type !== 'optionGroup');
  if (!hasMessages) return '';

  return `
    <div class="arg-chat-day-divider" aria-label="Messages from today">
      <span>Today</span>
    </div>
  `;
}

function renderPromptSuggestions() {
  const prompts = [
    'Explain what you can access',
    'Review recent logs for errors',
    'Check provider usage limits',
    'Help me secure this workspace',
  ];

  return `
    <div class="prompt-suggestion-row">
      ${prompts
        .map(
          (prompt) => `
            <button type="button" data-chat-prompt="${escapeAttribute(prompt)}">${escapeHtml(prompt)}</button>
          `,
        )
        .join('')}
    </div>
  `;
}

function renderInspector(state, provider) {
  const usageLine = formatUsageLine(state.usageSnapshot);
  const activeSession = state.chatSessions.find((chat) => chat.id === state.activeChatId);
  const messageCount = state.chatBlocks.filter((block) => block.type === 'message').length;
  const modelDisplay = displayModelName(state.providerModel);

  return `
    <aside class="inspector-panel arg-chat-inspector panel">
      <div class="panel-header split-header">
        <h3>Inspector</h3>
        <button class="icon-button compact" data-toggle-chat-panel="inspector" title="Hide inspector" aria-label="Hide inspector"><span data-icon="chevrons-right"></span></button>
      </div>
      <div class="inspector-tabs" role="tablist">
        <button class="active" type="button">Session</button>
        <button type="button">Workspace</button>
      </div>
      <section class="inspector-card">
        <span>Session info</span>
        <dl>
          <div><dt>Session ID</dt><dd>${escapeHtml(state.activeChatId)}</dd></div>
          <div><dt>Messages</dt><dd>${messageCount.toLocaleString()}</dd></div>
          <div><dt>Updated</dt><dd>${escapeHtml(activeSession?.lastMessageAt ? formatChatTime(activeSession.lastMessageAt) : 'Now')}</dd></div>
          <div><dt>Provider</dt><dd>${escapeHtml(provider.label)}</dd></div>
          <div><dt>Model</dt><dd>${escapeHtml(modelDisplay)}</dd></div>
          <div><dt>Usage</dt><dd>${escapeHtml(usageLine)}</dd></div>
        </dl>
      </section>
      <section class="inspector-card">
        <span>Approved context</span>
        <p>${escapeHtml((state.selectedContextAccess || []).join(', ') || 'No app context approved yet.')}</p>
      </section>
      <section class="inspector-card">
        <span>Workspace files</span>
        <p>File inventory appears here after you approve workspace context scanning.</p>
      </section>
      <section class="inspector-card security-status">
        <span>Security status</span>
        <strong>${escapeHtml(state.securityProfile === 'restricted' ? 'Restricted workspace' : state.securityProfile)}</strong>
        <p>No arbitrary shell execution from the GUI.</p>
      </section>
    </aside>
  `;
}

function renderChatTelemetry(state, provider, metadata, estimatedTokens, contextPercent) {
  const stats = state.desktopState?.systemStats || {};
  const gateway = state.desktopState?.gatewayPid
    ? `Gateway PID ${state.desktopState.gatewayPid}`
    : 'Gateway stopped';
  const memory =
    Number.isFinite(stats.memoryUsedPercent) && stats.memoryUsedPercent > 0
      ? `${Math.round(stats.memoryUsedPercent)}%`
      : 'Unavailable';
  const cpu =
    Number.isFinite(stats.cpuUsagePercent) && stats.cpuUsagePercent > 0
      ? `${Math.round(stats.cpuUsagePercent)}%`
      : 'Unavailable';
  const session = state.activeChatId || 'No session';
  const usageLine = formatUsageLine(state.usageSnapshot);

  return `
    <footer class="arg-telemetry-strip" aria-label="Chat telemetry">
      <div><span class="status-dot ${state.apiTest.status === 'ok' ? 'ok' : 'warn'}"></span><strong>${escapeHtml(provider.label)}</strong><small>${escapeHtml(state.apiTest.status === 'ok' ? 'Ready' : 'Offline')}</small></div>
      <div><span>Session</span><strong>${escapeHtml(session)}</strong><small>${state.chatBlocks.length.toLocaleString()} blocks</small></div>
      <div><span>Runtime</span><strong>${escapeHtml(gateway)}</strong><small>${escapeHtml(state.desktopState?.workspaceReady ? 'Workspace ready' : 'Workspace pending')}</small></div>
      <div><span>System</span><strong>CPU ${escapeHtml(cpu)} · RAM ${escapeHtml(memory)}</strong><small>${escapeHtml(stats.hostName || 'Host unavailable')}</small></div>
      <div><span>Usage</span><strong>${escapeHtml(usageLine)}</strong><small>${escapeHtml(new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }))}</small></div>
    </footer>
  `;
}

function renderNewTransmissionBar(state) {
  if (!state.chatHasNewTransmission) return '';

  return `
    <button type="button" class="chat-new-transmission" data-new-transmission="true">
      NEW TRANSMISSION
      <span data-icon="chevronDown"></span>
    </button>
  `;
}

function renderContextRing(estimatedTokens, metadata, contextPercent, usageSnapshot) {
  const limit = contextTokenLimit(metadata);
  const status = contextPercent >= 85 ? 'hot' : contextPercent >= 65 ? 'warm' : 'calm';
  const usageLine = formatUsageLine(usageSnapshot);

  return `
    <div class="context-usage-ring arg-context-ring ${status}" style="--context-percent:${contextPercent};" aria-label="Context ${contextPercent}% used" tabindex="0">
      <span class="context-ring-label">Context</span>
      <span class="context-ring-percent">${contextPercent}%</span>
      <div class="context-ring-popover" role="tooltip">
        <strong>Context</strong>
        <p>${estimatedTokens.toLocaleString()} estimated tokens used of ${limit.toLocaleString()}.</p>
        <p>${escapeHtml(metadata.currentContextLabel)}</p>
        <p>${escapeHtml(usageLine)}</p>
        <button type="button" class="button compact-context-button" data-compact-context="true">Compact context</button>
      </div>
    </div>
  `;
}

function formatUsageLine(usageSnapshot) {
  if (!usageSnapshot) return 'Usage unavailable from provider';
  if (usageSnapshot.summary) return usageSnapshot.summary;

  const cadence =
    usageSnapshot.requestResetCadence ||
    usageSnapshot.tokenResetCadence ||
    usageSnapshot.resetCadence ||
    'cadence not reported';
  const pieces = [];
  if (usageSnapshot.requestRemaining || usageSnapshot.requestLimit) {
    pieces.push(
      `Requests ${usageSnapshot.requestRemaining || '?'} / ${usageSnapshot.requestLimit || '?'}`,
    );
  }
  if (usageSnapshot.tokenRemaining || usageSnapshot.tokenLimit) {
    pieces.push(
      `Tokens ${usageSnapshot.tokenRemaining || '?'} / ${usageSnapshot.tokenLimit || '?'}`,
    );
  }
  if (usageSnapshot.requestReset || usageSnapshot.tokenReset) {
    pieces.push(`Reset ${usageSnapshot.requestReset || usageSnapshot.tokenReset} (${cadence})`);
  }
  return pieces.join(' · ') || 'Usage unavailable from provider';
}

function renderTypingIndicator(state) {
  if (!state.chatStreaming) return '';

  return `
    <div class="arg-typing-status typing-indicator" aria-live="polite">
      <span class="arg-typing-name">${escapeHtml(state.agentName || 'Argentum')}</span>
      <span>is typing</span>
      <span class="arg-typing-dots" aria-hidden="true"><span></span><span></span><span></span></span>
    </div>
  `;
}

function renderAttachmentTray(state) {
  if (state.chatAttachments.length === 0) return '';

  return `
    <div class="attachment-tray">
      ${state.chatAttachments.map((file) => renderAttachmentPreview(file)).join('')}
    </div>
  `;
}

function renderAttachmentPreview(file) {
  const kind = file.kind || (String(file.mime || '').startsWith('image/') ? 'image' : 'file');
  const preview =
    kind === 'image' && file.previewSrc
      ? `<img src="${escapeAttribute(file.previewSrc)}" alt="${escapeAttribute(file.name || 'Attached image')}" />`
      : `<span class="attachment-file-icon"><span data-icon="${kind === 'image' ? 'image' : 'file'}"></span></span>`;

  return `
    <article class="attachment-preview" title="${escapeAttribute(file.path || file.name || 'Attachment')}">
      ${preview}
      <span>
        <strong>${escapeHtml(file.name || 'Attachment')}</strong>
        <small>${escapeHtml(file.mime || kind)}</small>
      </span>
      ${
        file.removable === false
          ? ''
          : `<button class="icon-button compact" data-remove-attachment="${escapeAttribute(file.id || '')}" title="Remove attachment" aria-label="Remove attachment"><span data-icon="x"></span></button>`
      }
    </article>
  `;
}

function renderComposer(state, metadata, estimatedTokens, contextPercent) {
  const sendDisabled =
    state.chatStreaming || (!state.draftMessage.trim() && state.chatAttachments.length === 0);
  const supportsThinking = modelSupportsThinking(metadata);
  const sendStatus = state.chatStreaming
    ? 'Generation in progress. Stop it before sending another message.'
    : sendDisabled
      ? ''
      : 'Ready to send.';
  const sendTitle = state.chatStreaming
    ? 'Generation in progress. Stop it before sending another message.'
    : sendDisabled
      ? 'Enter a message or attach a file.'
      : 'Ready to send.';
  return `
    <div class="arg-composer-shell">
      <div class="conversation-composer composer">
        <div class="composer-status-line" id="send-chat-status">${escapeHtml(sendStatus)}</div>
        <div class="composer-layout" data-thinking="${supportsThinking ? 'true' : 'false'}">
          <div class="composer-inline">
            <button class="icon-button" id="attach-file" title="Attach file or image" aria-label="Attach file or image"><span data-icon="paperclip"></span></button>
            <button class="icon-button" id="voice-input" title="Use microphone" aria-label="Use microphone">${
              state.voiceInputStatus === 'listening'
                ? '<span class="typing-dots">...</span>'
                : '<span data-icon="mic"></span>'
            }</button>
            ${
              supportsThinking
                ? `<label class="compact-field">
                    <span>Thinking</span>
                    <select id="thinking-level" aria-label="Thinking level">
                      ${thinkingLevels
                        .map(
                          (level) => `
                            <option value="${escapeAttribute(level.id)}" ${selected(state.thinkingLevel, level.id)}>${escapeHtml(level.label)}</option>
                          `,
                        )
                        .join('')}
                    </select>
                  </label>`
                : ''
            }
            <textarea id="chat-draft" placeholder="Tell Argentum what to do. Attach files when useful.">${escapeHtml(state.draftMessage)}</textarea>
            ${renderContextRing(estimatedTokens, metadata, contextPercent, state.usageSnapshot)}
            ${
              state.chatStreaming
                ? `<button class="button danger button-icon-label" data-stop-generation="true" id="stop-chat"><span data-icon="stop"></span><span>Stop</span></button>`
                : `<button class="button primary button-icon-label" id="send-chat" data-send-chat-button="true" aria-describedby="send-chat-status" title="${escapeAttribute(sendTitle)}" ${sendDisabled ? 'disabled' : ''}><span data-icon="send"></span><span>Send</span></button>`
            }
          </div>
        </div>
        ${renderAttachmentTray(state)}
      </div>
    </div>
  `;
}

function renderReasoningPanel(block, state) {
  if (!state.showThinkingInChat || !block.reasoning) return '';

  return `
    <details class="reasoning-panel arg-trace-note">
      <summary>Trace note</summary>
      <div class="markdown-body">${renderMarkdown(block.reasoning)}</div>
    </details>
  `;
}

function renderChatBlock(block, state) {
  return renderMessageComponent(block, state);
}

function renderMessageComponent(block, state) {
  if (block.type === 'optionGroup') {
    return `
      <article class="chat-block option-group">
        <span>${escapeHtml(block.title)}</span>
        <div class="markdown-body">${renderMarkdown(block.body)}</div>
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
      <div class="message-row arg-chat-line arg-chat-line-system system">
        <article class="message system arg-chat-system-line chat-block ${escapeAttribute(block.type)}">
          <span class="arg-status-rail" aria-hidden="true"></span>
          <div class="arg-chat-system-copy">
            <strong>${escapeHtml(block.title)}</strong>
            <div class="markdown-body">${renderMarkdown(block.body)}</div>
          </div>
        </article>
      </div>
    `;
  }

  const roleClass =
    block.role === 'user' ? 'user' : block.role === 'system' ? 'system' : 'assistant';
  const messageId = escapeAttribute(block.id || '');
  const title = block.title || (block.role === 'user' ? 'You' : state.agentName || 'Argentum');
  const rowClass =
    roleClass === 'user'
      ? 'arg-chat-line-user'
      : roleClass === 'system'
        ? 'arg-chat-line-system'
        : 'arg-chat-line-assistant';
  const initials = roleClass === 'user' ? userInitials(state.userName || title) : 'A';
  const userAvatarSrc =
    roleClass === 'user' && state.userAvatarPath ? filePreviewUrl(state.userAvatarPath) : '';
  const assistantAvatar = `<img src="./assets/argentum.png" alt="" loading="lazy" />`;
  const userAvatar = userAvatarSrc
    ? `<img src="${escapeAttribute(userAvatarSrc)}" alt="" loading="lazy" />`
    : escapeHtml(initials);
  const time = formatChatTime(block.createdAt || Date.now());

  return `
    <div class="message-row arg-chat-line ${rowClass} ${roleClass}">
      ${roleClass === 'assistant' ? `<div class="arg-chat-avatar arg-chat-avatar-assistant" aria-hidden="true">${assistantAvatar}</div>` : ''}
      <article class="message ${roleClass} arg-chat-content ${escapeAttribute(block.status || '')}" data-message-id="${messageId}">
        <div class="message-meta arg-chat-meta">
          <strong class="arg-chat-author">${escapeHtml(title)}</strong>
          <time>${escapeHtml(time)}</time>
          <div class="message-actions">${renderMessageActions(block)}</div>
        </div>
        ${renderMessageAttachments(block.attachments || [])}
        <div class="markdown-body">${renderMarkdown(block.body)}</div>
        ${renderReasoningPanel(block, state)}
      </article>
      ${roleClass === 'user' ? `<div class="arg-chat-avatar arg-chat-avatar-user" aria-hidden="true">${userAvatar}</div>` : ''}
    </div>
  `;
}

function userInitials(value) {
  const text = String(value || 'You').trim();
  const parts = text.split(/\s+/).filter(Boolean);
  const initials = parts.length > 1 ? `${parts[0][0]}${parts[1][0]}` : text.slice(0, 2);
  return initials.toUpperCase();
}

function renderMessageAttachments(attachments) {
  if (!Array.isArray(attachments) || attachments.length === 0) return '';
  return `
    <div class="message-attachments">
      ${attachments.map((attachment) => renderAttachmentPreview({ ...attachment, removable: false })).join('')}
    </div>
  `;
}

function renderMessageActions(block) {
  if (!block.id) return '';
  const id = escapeAttribute(block.id);
  const actions = [
    `<button class="icon-button compact" data-copy-message="${id}" title="Copy message" aria-label="Copy message"><span data-icon="copy"></span></button>`,
  ];
  if (block.role !== 'user') {
    actions.push(
      `<button class="icon-button compact" data-regenerate-message="${id}" title="Regenerate response" aria-label="Regenerate response"><span data-icon="refresh"></span></button>`,
    );
  }
  if (block.status === 'error') {
    actions.push(
      `<button class="icon-button compact" data-retry-message="${id}" title="Retry message" aria-label="Retry message"><span data-icon="refresh"></span></button>`,
    );
  }
  return actions.join('');
}
