import { modelMetadata, providerPresets, thinkingLevels } from './constants.js';
import { renderNotifications } from './shell.js';
import {
  contextTokenLimit,
  contextUsagePercent,
  currentProvider,
  displayModelName,
  escapeAttribute,
  escapeHtml,
  estimateRuntimeContextTokens,
  modelMetadataFor,
  modelOptionsFor,
  renderMarkdown,
} from './utils.js';

export const chatModule = {
  id: 'chat',
  label: 'Tasks',
  render: renderChat,
  validate: () => '',
  healthCheck: (state) => ({
    status: state.apiTest.status === 'error' ? 'degraded' : 'ok',
    message:
      state.apiTest.status === 'error'
        ? 'Tasks are available in offline mode while provider access is repaired.'
        : 'Task workspace ready.',
  }),
};

function renderChat(state) {
  const provider = currentProvider(providerPresets, state);
  const model = modelMetadataFor(state.providerModel, modelMetadata);
  const providerContext = numberFromUsage(state.usageSnapshot?.contextTokens);
  const providerLimit = numberFromUsage(state.usageSnapshot?.contextTokenLimit);
  const localContext = estimateRuntimeContextTokens(state, state.draftMessage);
  const effectiveModel =
    providerLimit > 0
      ? { ...model, maxContextTokens: providerLimit }
      : model;
  const usedContext = providerContext > 0 ? Math.max(localContext, providerContext) : localContext;
  const contextPercent = contextUsagePercent(usedContext, effectiveModel);
  const activeConversation =
    state.chatSessions.find((conversation) => conversation.id === state.activeChatId) ||
    state.chatSessions[0];
  const hasUserMessage = state.chatBlocks.some((block) => block.role === 'user');
  const classes = [
    'chat-layout',
    state.conversationsCollapsed ? 'history-is-closed' : 'history-is-open',
    state.inspectorCollapsed ? 'details-is-closed' : 'details-is-open',
  ].join(' ');

  return `
    ${renderNotifications()}
    <div class="${classes}">
      ${renderConversationLibrary(state)}
      ${
        !state.conversationsCollapsed || !state.inspectorCollapsed
          ? '<button class="chat-panel-scrim" type="button" data-close-chat-panels="true" aria-label="Close task panels"></button>'
          : ''
      }

      <section class="conversation-stage">
        <header class="conversation-header">
          <div class="thread-heading">
            <button
              class="icon-button tactile-control thread-sidebar-trigger"
              type="button"
              data-toggle-chat-panel="conversations"
              title="${state.conversationsCollapsed ? 'Show task list' : 'Hide task list'}"
              aria-label="${state.conversationsCollapsed ? 'Show task list' : 'Hide task list'}"
            ><span data-icon="panelLeft"></span></button>
            <div class="thread-title">
              <span class="thread-kicker">Active task</span>
              <h2>${escapeHtml(activeConversation?.title || 'New task')}</h2>
              <button
                class="thread-workspace"
                type="button"
                data-workspace-menu="true"
                title="${escapeAttribute(state.workspacePath || 'Choose a workspace')}"
                aria-label="${escapeAttribute(`Workspace: ${workspaceDisplayName(state.workspacePath)}`)}"
                aria-haspopup="menu"
                aria-expanded="${state.workspaceMenuOpen ? 'true' : 'false'}"
              >
                <span data-icon="folder"></span>
                <span>${escapeHtml(workspaceDisplayName(state.workspacePath))}</span>
                <span data-icon="chevronDown"></span>
              </button>
            </div>
          </div>

          <div class="conversation-header-center" aria-label="Task status">
            <span class="run-state ${state.chatStreaming ? 'is-running' : 'is-ready'}">
              <span class="run-state-dot" aria-hidden="true"></span>
              ${state.chatStreaming ? 'Running' : 'Ready'}
            </span>
            <span class="header-note">${escapeHtml(state.runtimeMode === 'desktop' ? 'Desktop run' : `${state.runtimeMode} run`)}</span>
          </div>

          <div class="conversation-header-actions">
            <button
              class="icon-button tactile-control"
              type="button"
              data-toggle-chat-panel="inspector"
              title="${state.inspectorCollapsed ? 'Show run inspector' : 'Hide run inspector'}"
              aria-label="${state.inspectorCollapsed ? 'Show run inspector' : 'Hide run inspector'}"
            ><span data-icon="panelRight"></span></button>
            <button class="button quiet compact-header-action" type="button" data-new-chat="true">
              <span data-icon="plus"></span><span>New task</span>
            </button>
          </div>
        </header>

        ${renderTaskStatusStrip(state, provider, effectiveModel)}

        <div class="chat-transcript" aria-live="polite">
          <div class="message-stream">
            ${renderEmptyConversation(state, hasUserMessage)}
            ${hasUserMessage ? state.chatBlocks.map((block) => renderMessage(block, state)).join('') : ''}
          </div>
        </div>

        ${renderNewMessageNotice(state)}
        ${renderTypingStatus(state)}
        ${renderComposer(state, provider, effectiveModel, usedContext, contextPercent)}
      </section>

      ${renderDetailsPanel(state, provider, effectiveModel, usedContext, contextPercent)}
    </div>
  `;
}

function renderTaskStatusStrip(state, provider, model) {
  const permission = state.securityProfile || 'restricted';
  const modelLabel = displayModelName(state.providerModel);
  const connectionLabel = state.apiTest.status === 'ok' ? 'Connected' : 'Not tested';
  return `
    <div class="task-status-strip" aria-label="Run configuration">
      <div class="task-status-item">
        <span class="status-mark status-mark-branch" aria-hidden="true">⌁</span>
        <span><small>Workspace</small><strong>${escapeHtml(workspaceDisplayName(state.workspacePath))}</strong></span>
      </div>
      <span class="status-strip-divider" aria-hidden="true"></span>
      <div class="task-status-item">
        <span class="status-mark status-mark-model" aria-hidden="true">A</span>
        <span><small>${escapeHtml(provider.label)}</small><strong>${escapeHtml(modelLabel)}</strong></span>
      </div>
      <span class="status-strip-divider" aria-hidden="true"></span>
      <div class="task-status-item">
        <span class="status-mark status-mark-lock" data-icon="lock" aria-hidden="true"></span>
        <span><small>Permissions</small><strong>${escapeHtml(permission)}</strong></span>
      </div>
      <span class="status-strip-spacer"></span>
      <span class="connection-label ${state.apiTest.status === 'ok' ? 'is-ready' : ''}"><span aria-hidden="true"></span>${escapeHtml(connectionLabel)}</span>
      <span class="sr-only">${escapeHtml(model.detail || '')}</span>
    </div>
  `;
}

function renderConversationLibrary(state) {
  const conversations = filteredConversations(state);
  let currentGroup = '';
  const items = conversations
    .map((conversation) => {
      const group = conversationGroup(conversation.updatedAt || conversation.lastMessageAt);
      const heading = group !== currentGroup ? `<p class="conversation-group-label">${escapeHtml(group)}</p>` : '';
      currentGroup = group;
      const active = conversation.id === state.activeChatId;
      return `
        ${heading}
        <article class="conversation-list-item ${active ? 'is-active' : ''}" data-conversation-card="${escapeAttribute(conversation.id)}">
          <button class="conversation-open-button" type="button" data-recent-chat="${escapeAttribute(conversation.id)}" ${active ? 'aria-current="page"' : ''}>
            <span class="conversation-icon ${conversation.channel === 'telegram' ? 'is-channel' : ''}" aria-hidden="true">${conversation.channel === 'telegram' ? '↗' : 'A'}</span>
            <span class="conversation-copy"><strong>${escapeHtml(conversation.title || 'Untitled task')}</strong><span>${escapeHtml(conversation.subtitle || 'No messages yet')}</span></span>
            ${conversation.unreadCount ? `<span class="conversation-unread">${conversation.unreadCount}</span>` : ''}
          </button>
          ${conversation.pinned ? '<span class="pinned-mark" data-icon="pin" aria-label="Pinned"></span>' : ''}
          <button class="conversation-menu-button" type="button" data-conversation-menu="${escapeAttribute(conversation.id)}" aria-label="Task actions"><span data-icon="moreVertical"></span></button>
          ${renderConversationMenu(conversation, state)}
          ${
            state.pendingDeleteChatId === conversation.id
              ? `<div class="conversation-delete-confirm" role="alert"><p>Delete this local task?</p><button class="button danger" type="button" data-confirm-delete-chat="${escapeAttribute(conversation.id)}">Delete</button><button class="button quiet" type="button" data-cancel-delete-chat="true">Cancel</button></div>`
              : ''
          }
        </article>
      `;
    })
    .join('');

  return `
    <aside class="conversation-library task-sidebar" aria-label="Tasks">
      <div class="library-header">
        <div class="task-sidebar-brand"><span class="library-eyebrow">Sessions</span><strong>Recent work</strong></div>
        <button class="icon-button compact task-sidebar-close" type="button" data-toggle-chat-panel="conversations" title="Hide task list" aria-label="Hide task list"><span data-icon="panelLeft"></span></button>
      </div>
      <div class="library-search" role="search"><span data-icon="search" aria-hidden="true"></span><span>Find a task</span><kbd>/</kbd></div>
      <button class="sidebar-new-task" type="button" data-new-chat="true"><span data-icon="plus"></span><span>New task</span><span class="new-task-hint">Ctrl+N</span></button>
      <button class="sidebar-workspace" type="button" data-workspace-menu="true" title="${escapeAttribute(state.workspacePath || 'Choose a workspace')}" aria-label="${escapeAttribute(`Workspace: ${workspaceDisplayName(state.workspacePath)}`)}" aria-haspopup="menu" aria-expanded="${state.workspaceMenuOpen ? 'true' : 'false'}">
        <span class="workspace-tile-icon" data-icon="folder"></span><span><small>Workspace</small><strong>${escapeHtml(workspaceDisplayName(state.workspacePath))}</strong></span><span data-icon="chevronDown"></span>
      </button>
      <div class="conversation-filter" role="tablist" aria-label="Task filter">
        <button type="button" class="${state.chatFilter === 'all' ? 'is-active' : ''}" data-chat-filter="all">All tasks</button>
        <button type="button" class="${state.chatFilter === 'pinned' ? 'is-active' : ''}" data-chat-filter="pinned">Pinned</button>
      </div>
      <div class="conversation-list">${items || '<p class="empty-list-copy">No tasks in this view.</p>'}</div>
      <footer class="task-sidebar-footer">
        <button type="button" data-section="settings"><span data-icon="settings"></span><span>Workspace settings</span><span class="footer-shortcut">Ctrl+,</span></button>
      </footer>
    </aside>
  `;
}

function renderConversationMenu(conversation, state) {
  if (state.conversationMenuChatId !== conversation.id) return '';
  const id = escapeAttribute(conversation.id);
  return `<div class="conversation-menu" role="menu">
    <button type="button" data-pin-chat="${id}"><span data-icon="pin"></span>${conversation.pinned ? 'Unpin' : 'Pin'} task</button>
    <button type="button" data-rename-chat="${id}"><span data-icon="edit"></span>Rename</button>
    <button type="button" data-clear-chat="${id}"><span data-icon="refresh"></span>Clear messages</button>
    <button class="danger-text" type="button" data-delete-chat="${id}"><span data-icon="trash"></span>Delete</button>
  </div>`;
}

function renderEmptyConversation(state, hasUserMessage) {
  if (hasUserMessage) return '';
  const workspaceLabel = workspaceDisplayName(state.workspacePath);
  const prompts = [
    { label: 'Explain this workspace', detail: 'Find the important files and scripts.', prompt: 'Inspect this workspace and explain its structure, scripts, and important conventions.' },
    { label: 'Fix a problem', detail: 'Follow the evidence and verify a fix.', prompt: 'Check the local runtime for errors, fix the most likely cause, and verify the result.' },
    { label: 'Build a feature', detail: 'Plan the change, then build it.', prompt: 'Help me plan and build a new feature in this workspace, then verify that it works.' },
  ];
  return `
    <section class="task-brief chat-welcome">
      <div class="task-brief-mark argentum-mark large" aria-hidden="true"><span>AG</span><i></i><i></i><i></i></div>
      <div class="task-brief-copy"><span class="section-kicker">${escapeHtml(workspaceLabel)} · local run</span><h3 class="empty-state-heading">What should we work on?</h3><p>Describe the outcome. Argentum will inspect the workspace, show its work, and keep you in control of changes.</p></div>
      <div class="prompt-grid" aria-label="Suggested tasks">
        ${prompts.map((item) => `<button type="button" data-chat-prompt="${escapeAttribute(item.prompt)}"><span class="prompt-index">0${prompts.indexOf(item) + 1}</span><span><strong>${escapeHtml(item.label)}</strong><small>${escapeHtml(item.detail)}</small></span><span data-icon="chevrons-right" aria-hidden="true"></span></button>`).join('')}
      </div>
      <p class="task-brief-footnote"><span class="live-indicator" aria-hidden="true"></span> Workspace boundary active · ${escapeHtml(state.securityProfile || 'restricted')} permissions</p>
    </section>
  `;
}

function renderMessage(block, state) {
  if (block.type === 'optionGroup') {
    return `<article class="system-card option-card-group"><div class="system-card-heading"><span class="activity-icon">?</span><div><strong>${escapeHtml(block.title || 'Choose an option')}</strong><div class="markdown-body">${renderMarkdown(block.body || '')}</div></div></div><div class="option-grid">${(block.options || []).map((option) => `<button type="button" data-chat-option="${escapeAttribute(option.id)}"><strong>${escapeHtml(option.label)}</strong><span>${escapeHtml(option.detail)}</span></button>`).join('')}</div></article>`;
  }
  if (['warning', 'summary', 'actionCard'].includes(block.type)) {
    return `<article class="system-card ${escapeAttribute(block.type)}"><span class="system-card-line" aria-hidden="true"></span><div><strong>${escapeHtml(block.title || 'Workspace note')}</strong><div class="markdown-body">${renderMarkdown(block.body || '')}</div></div></article>`;
  }

  const isUser = block.role === 'user';
  const isSystem = block.role === 'system';
  const role = isUser ? 'user' : isSystem ? 'system' : 'assistant';
  const name = block.title || (isUser ? state.userName || 'You' : state.agentName || 'Argentum');
  const status = block.status || '';
  const hasToolSignal = !isUser && status === 'streaming';
  return `
    <article class="chat-message ${role} ${escapeAttribute(status)}" data-message-id="${escapeAttribute(block.id || '')}" data-status="${escapeHtml(status)}">
      <div class="message-marker message-avatar ${role}" aria-hidden="true">${role === 'assistant' ? 'A' : role === 'user' ? initials(name) : '!'}</div>
      <div class="message-body">
        <header><div class="message-author"><strong>${escapeHtml(name)}</strong>${hasToolSignal ? '<span class="message-live-dot"></span><small>working</small>' : ''}</div><time>${escapeHtml(formatChatTime(block.createdAt))}</time><div class="message-actions">${renderMessageActions(block)}</div></header>
        ${renderMessageAttachments(block.attachments || [])}
        <div class="markdown-body">${renderMarkdown(block.body || '')}</div>
        ${renderReasoning(block, state)}
      </div>
    </article>
  `;
}

function renderMessageActions(block) {
  if (!block.id) return '';
  const id = escapeAttribute(block.id);
  return `<button class="icon-button compact" type="button" data-copy-message="${id}" title="Copy message" aria-label="Copy message"><span data-icon="copy"></span></button>${block.role !== 'user' && block.status !== 'error' ? `<button class="icon-button compact" type="button" data-regenerate-message="${id}" title="Regenerate response" aria-label="Regenerate response"><span data-icon="refresh"></span></button>` : ''}${block.status === 'error' ? `<button class="icon-button compact" type="button" data-retry-message="${id}" title="Retry message" aria-label="Retry message"><span data-icon="retry"></span></button>` : ''}`;
}

function renderMessageAttachments(attachments) {
  if (!Array.isArray(attachments) || attachments.length === 0) return '';
  return `<div class="message-attachments">${attachments.map((attachment) => `<span class="message-attachment"><span data-icon="${attachment.kind === 'image' ? 'image' : 'file'}"></span>${escapeHtml(attachment.name || 'Attachment')}</span>`).join('')}</div>`;
}

function renderReasoning(block, state) {
  if (!state.showThinkingInChat || !block.reasoning) return '';
  return `<details class="reasoning-disclosure"><summary>Show reasoning details</summary><div class="markdown-body">${renderMarkdown(block.reasoning)}</div></details>`;
}

function renderComposer(state, provider, model, usedContext, contextPercent) {
  const disabled = state.chatStreaming || (!state.draftMessage.trim() && state.chatAttachments.length === 0);
  const thinking = thinkingLevels.find((level) => level.id === state.thinkingLevel);
  const contextLabel = formatContextPercent(contextPercent, usedContext);
  return `
    <footer class="composer-region ${state.chatControlsOpen ? 'chat-controls-are-open' : ''}">
      ${state.chatControlsOpen ? renderChatControlPicker(state, provider, model, usedContext, contextPercent) : ''}
      <div class="composer-caption"><span>Describe the next step</span><span><kbd>Shift</kbd><kbd>Enter</kbd> for a new line</span></div>
      <div class="composer-card">
        ${renderAttachmentTray(state)}
          <textarea id="chat-draft" rows="1" placeholder="Ask Argentum to inspect, change, or explain something" aria-label="Message Argentum">${escapeHtml(state.draftMessage)}</textarea>
          <div class="composer-toolbar">
            <div class="composer-tools">
            <details class="composer-tool-disclosure">
              <summary class="composer-tool-toggle" title="Open task tools" aria-label="Open task tools" aria-controls="composer-tool-menu"><span data-icon="plus"></span><span class="composer-tool-toggle-copy">Tools</span></summary>
              <div class="composer-tool-menu" id="composer-tool-menu" role="menu" aria-label="Task tools">
                <button class="composer-menu-action" id="attach-file" type="button" role="menuitem" title="Attach a file"><span class="composer-menu-icon" data-icon="paperclip"></span><span><strong>Attach file</strong><small>Add a file to this task</small></span></button>
                <button class="composer-menu-action" id="voice-input" type="button" role="menuitem" title="Voice input"><span class="composer-menu-icon">${state.voiceInputStatus === 'listening' ? '<span class="listening-dot"></span>' : '<span data-icon="mic"></span>'}</span><span><strong>Voice input</strong><small>${state.voiceInputStatus === 'listening' ? 'Listening now' : 'Dictate a task'}</small></span></button>
                <div class="composer-tool-menu-divider" aria-hidden="true"></div>
                <button class="composer-menu-action" type="button" role="menuitem" data-open-chat-details="true" title="Open run inspector"><span class="composer-menu-icon" data-icon="panelRight"></span><span><strong>Open inspector</strong><small>Review activity and access</small></span></button>
              </div>
            </details>
            <button class="workspace-control" type="button" data-workspace-menu="true" title="${escapeAttribute(state.workspacePath || 'Choose a workspace')}" aria-label="${escapeAttribute(`Workspace: ${workspaceDisplayName(state.workspacePath)}`)}" aria-haspopup="menu" aria-expanded="${state.workspaceMenuOpen ? 'true' : 'false'}"><span data-icon="folder" aria-hidden="true"></span><span class="workspace-control-copy">${escapeHtml(workspaceDisplayName(state.workspacePath))}</span></button>
            <button class="chat-runtime-pill" type="button" data-toggle-chat-controls="true" aria-haspopup="dialog" aria-expanded="${state.chatControlsOpen ? 'true' : 'false'}" aria-label="${escapeAttribute(`Configure run: ${provider.label}, ${displayModelName(state.providerModel)}, ${thinking?.label || 'Balanced'} thinking, ${contextLabel} context`)}" title="Configure model and thinking"><span class="runtime-pill-signal" aria-hidden="true"></span><strong>${escapeHtml(displayModelName(state.providerModel))}</strong><span class="runtime-pill-divider" aria-hidden="true"></span><span>${escapeHtml(thinking?.label || 'Balanced')}</span><span class="composer-context-usage">${escapeHtml(contextLabel)} context</span><span data-icon="chevronUp" aria-hidden="true"></span></button>
          </div>
          <div class="composer-actions"><span class="composer-send-status" id="send-chat-status" data-state="${state.chatStreaming ? 'busy' : disabled ? 'blocked' : 'ready'}" role="status">${state.chatStreaming ? 'Working' : ''}</span>${state.chatStreaming ? `<button class="send-button is-stop" type="button" id="stop-chat" data-stop-generation="true" title="Stop generation" aria-label="Stop generation"><span data-icon="stop"></span></button>` : `<button class="send-button" type="button" id="send-chat" data-send-chat-button="true" aria-describedby="send-chat-status" title="Send task" aria-label="Send message" ${disabled ? 'disabled' : ''}><span data-icon="send" aria-hidden="true"></span></button>`}</div>
        </div>
      </div>
      <p class="composer-trust"><span data-icon="shield" aria-hidden="true"></span> Argentum can act inside the selected workspace. You approve the boundary.</p>
    </footer>
  `;
}

function renderAttachmentTray(state) {
  if (!state.chatAttachments.length) return '';
  return `<div class="attachment-tray">${state.chatAttachments.map((attachment) => `<article class="attachment-item"><span class="attachment-icon" data-icon="${attachment.kind === 'image' ? 'image' : 'file'}"></span><span><strong>${escapeHtml(attachment.name || 'Attachment')}</strong><small>${escapeHtml(attachment.mime || attachment.kind || 'file')}</small></span><button class="icon-button compact" type="button" data-remove-attachment="${escapeAttribute(attachment.id || '')}" aria-label="Remove attachment"><span data-icon="x"></span></button></article>`).join('')}</div>`;
}

function renderChatControlPicker(state, provider, model, usedContext, contextPercent) {
  const options = modelOptionsFor(provider, state.providerModel, state.providerAuthMethod).filter((option) => option.id !== 'custom-model');
  const contextLimit = contextTokenLimit(model);
  const contextLabel = formatContextPercent(contextPercent, usedContext);
  const contextLevel = Math.max(0, Math.min(100, Math.round(contextPercent)));
  return `
    <button class="chat-control-scrim" type="button" data-close-chat-controls="true" aria-label="Close run configuration"></button>
    <aside class="chat-control-popover runtime-picker" role="dialog" aria-label="Model and thinking">
      <header class="chat-control-header"><div><span class="section-kicker">Run configuration</span><h2>How should this task run?</h2><p><span class="runtime-connection-dot ${state.apiTest.status === 'ok' ? 'is-ready' : ''}"></span>${escapeHtml(provider.label)} · ${escapeHtml(state.runtimeMode)}</p></div><button class="icon-button compact" type="button" data-close-chat-controls="true" aria-label="Close run configuration"><span data-icon="x"></span></button></header>
      <div class="chat-control-grid"><section class="model-control-section"><div class="chat-control-section-heading"><strong>Model</strong><span class="core-status ${state.apiTest.status === 'ok' ? 'is-ready' : ''}">${state.apiTest.status === 'ok' ? 'Connected' : 'Not tested'}</span></div><div class="chat-model-list" role="listbox" aria-label="${escapeAttribute(`${provider.label} models`)}">${options.map((option) => { const metadata = modelMetadataFor(option.id, modelMetadata); const active = option.id === state.providerModel; return `<button class="chat-model-option ${active ? 'is-active' : ''}" type="button" role="option" aria-selected="${active ? 'true' : 'false'}" data-chat-model="${escapeAttribute(option.id)}"><span class="model-option-badge">${active ? '✓' : '·'}</span><span class="chat-model-copy"><strong>${escapeHtml(option.label || displayModelName(option.id))}</strong><small>${escapeHtml(metadata.detail || 'Provider model')}</small></span></button>`; }).join('')}</div></section><section class="core-tuning-section"><div class="chat-control-section-heading"><strong>Thinking</strong><span>Choose the effort for this run</span></div><div class="thinking-level-grid" role="group" aria-label="Thinking level">${thinkingLevels.map((level) => `<button class="${state.thinkingLevel === level.id ? 'is-active' : ''}" type="button" data-chat-thinking="${escapeAttribute(level.id)}" aria-pressed="${state.thinkingLevel === level.id ? 'true' : 'false'}" title="${escapeAttribute(level.detail)}"><strong>${escapeHtml(level.label)}</strong><small>${escapeHtml(level.detail)}</small></button>`).join('')}</div><div class="context-instrument context-summary"><div class="context-readout"><span>Estimated context</span><strong>${escapeHtml(contextLabel)}</strong></div><div class="context-ruler" role="progressbar" aria-label="Estimated context usage" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${contextLevel}"><span style="width:${contextLevel}%"></span></div><p>${usedContext.toLocaleString()} of ${contextLimit.toLocaleString()} estimated tokens</p></div></section></div>
      <footer class="chat-control-footer"><button type="button" data-open-chat-details="true">Open inspector</button><button type="button" data-compact-context="true">Compact context</button><button type="button" data-section="settings">Provider settings</button></footer>
    </aside>
  `;
}

function renderTypingStatus(state) {
  if (!state.chatStreaming) return '';
  return `<div class="typing-status" aria-live="polite"><span class="typing-pulse" aria-hidden="true"></span>${escapeHtml(state.agentName || 'Argentum')} is working through the task</div>`;
}

function renderDetailsPanel(state, provider, model, usedContext, contextPercent) {
  const activeConversation = state.chatSessions.find((conversation) => conversation.id === state.activeChatId);
  const messageCount = state.chatBlocks.filter((block) => block.type === 'message').length;
  const contextLimit = contextTokenLimit(model);
  const entries = Array.isArray(state.terminalEntries) ? state.terminalEntries.slice(-3).reverse() : [];
  return `
    <aside class="conversation-details" aria-label="Run inspector">
      <header><div><span class="section-kicker">Inspector</span><h2>Run details</h2><p>${escapeHtml(activeConversation?.title || 'New task')}</p></div><button class="icon-button" type="button" data-toggle-chat-panel="inspector" title="Hide run inspector" aria-label="Hide run inspector"><span data-icon="x"></span></button></header>
      <section class="detail-section inspector-summary"><div class="inspector-status"><span class="run-state-dot ${state.chatStreaming ? 'is-running' : 'is-ready'}"></span><strong>${state.chatStreaming ? 'Agent is working' : 'Ready to run'}</strong></div><p>${escapeHtml(state.actionStatus || 'Waiting for the next task.')}</p></section>
      <section class="detail-section"><div class="detail-heading"><h3>Execution</h3><span class="detail-badge">${escapeHtml(state.runtimeMode)}</span></div><dl class="definition-list compact"><div><dt>Provider</dt><dd>${escapeHtml(provider.label)}</dd></div><div><dt>Model</dt><dd>${escapeHtml(displayModelName(state.providerModel))}</dd></div><div><dt>Thinking</dt><dd>${escapeHtml(state.thinkingLevel || 'balanced')}</dd></div><div><dt>Permissions</dt><dd>${escapeHtml(state.securityProfile || 'restricted')}</dd></div></dl></section>
      <section class="detail-section"><div class="detail-heading"><h3>Context</h3><strong>${formatContextPercent(contextPercent, usedContext)}</strong></div><div class="context-meter"><span style="width:${Math.min(contextPercent, 100)}%"></span></div><p>${usedContext.toLocaleString()} of ${contextLimit.toLocaleString()} estimated tokens</p><button class="button quiet wide" type="button" data-compact-context="true">Compact context</button></section>
      <section class="detail-section"><div class="detail-heading"><h3>Workspace access</h3><span class="detail-badge is-secure">Scoped</span></div><div class="access-boundary"><span data-icon="shield"></span><span><strong>${escapeHtml(workspaceDisplayName(state.workspacePath))}</strong><small>Files inside the selected boundary</small></span></div><p class="detail-muted">${escapeHtml((state.selectedContextAccess || []).join(', ') || 'No optional context approved.')}</p></section>
      <details class="detail-section activity-section"><summary class="detail-heading"><h3>Activity</h3><span>${messageCount} messages <span data-icon="chevronDown" aria-hidden="true"></span></span></summary><div class="activity-content">${entries.length ? entries.map((entry) => `<div class="activity-row"><span class="activity-row-dot"></span><span><strong>${escapeHtml(entry.command || entry.event || 'Workspace action')}</strong><small>${escapeHtml(entry.output || entry.message || entry.status || 'Recorded')}</small></span></div>`).join('') : '<p class="detail-muted">Your next action will appear here.</p>'}</div></details>
      <footer class="inspector-footer"><button class="button quiet wide" type="button" data-section="settings">Workspace settings</button></footer>
    </aside>
  `;
}

function renderNewMessageNotice(state) {
  if (!state.chatHasNewTransmission) return '';
  return `<button class="new-message-notice" type="button" data-new-transmission="true">New activity <span data-icon="chevronDown"></span></button>`;
}

function filteredConversations(state) {
  const conversations = Array.isArray(state.chatSessions) ? state.chatSessions : [];
  return state.chatFilter === 'pinned' ? conversations.filter((conversation) => conversation.pinned) : conversations;
}

function conversationGroup(timestamp) {
  const now = new Date();
  const date = new Date(timestamp || Date.now());
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const day = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const difference = today - day;
  if (difference <= 0) return 'Today';
  if (difference <= 86400000) return 'Yesterday';
  if (difference <= 604800000) return 'This week';
  return 'Earlier';
}

function workspaceDisplayName(path) {
  const normalized = String(path || '').trim().replace(/[\\/]+$/, '');
  if (!normalized) return 'Choose workspace';
  return normalized.split(/[\\/]/).filter(Boolean).pop() || normalized;
}

function numberFromUsage(value) {
  const parsed = Number(String(value || '').replace(/[^\d.]/g, ''));
  return Number.isFinite(parsed) ? parsed : 0;
}

function formatContextPercent(contextPercent, usedContext = 0) {
  const rounded = Math.max(0, Math.min(100, Math.round(Number(contextPercent) || 0)));
  if (usedContext > 0 && rounded === 0) return '<1%';
  return `${rounded}%`;
}

function formatChatTime(timestamp) {
  return new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(new Date(timestamp || Date.now()));
}

function initials(name) {
  return String(name || 'You').trim().split(/\s+/).slice(0, 2).map((part) => part[0] || '').join('').toUpperCase() || 'Y';
}
