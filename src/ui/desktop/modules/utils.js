export function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => {
    const entities = {
      '&': '&amp;',
      '<': '&lt;',
      '>': '&gt;',
      '"': '&quot;',
      "'": '&#39;',
    };
    return entities[character];
  });
}

export function escapeAttribute(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}

export function selected(currentValue, optionValue) {
  return currentValue === optionValue ? 'selected' : '';
}

export function checked(values, optionValue) {
  return values.includes(optionValue) ? 'checked' : '';
}

export function labelFor(items, value) {
  return items.find((item) => item.id === value)?.label || value;
}

export function currentProvider(providers, state) {
  return providers.find((provider) => provider.id === state.llmProvider) || providers[0];
}

export function modelOptionsFor(provider, currentModel = '', authMethod = 'api-key') {
  const source =
    authMethod === 'browser-account' && provider.codexModels?.length
      ? provider.codexModels
      : provider.models || [{ id: provider.defaultModel, label: provider.defaultModel }];
  const options = [...source];
  if (!options.some((option) => option.id === 'custom-model')) {
    options.push({ id: 'custom-model', label: 'Other / custom model ID' });
  }
  const current = String(currentModel || '').trim();
  if (current && !options.some((option) => option.id === current)) {
    options.push({ id: current, label: `Current saved: ${current}` });
  }
  return options;
}

export function defaultModelForAuth(provider, authMethod = 'api-key') {
  if (authMethod === 'browser-account') {
    return provider.codexDefaultModel || provider.codexModels?.[0]?.id || provider.defaultModel;
  }

  return provider.defaultModel;
}

export function modelAllowedForAuth(provider, model, authMethod = 'api-key') {
  const cleanModel = String(model || '').trim();
  if (!cleanModel) return false;
  if (cleanModel !== '__custom__') return true;
  return modelOptionsFor(provider, '', authMethod).some((option) => option.id === cleanModel);
}

export function modelMetadataFor(modelId, metadata = {}) {
  return (
    metadata[modelId] || {
      contextWindow: 'Unknown',
      maxContextWindow: 'Unknown',
      currentContextLabel: 'Test provider to confirm limits',
      capabilities: ['chat'],
      detail: 'No baked-in metadata yet. Argentum will still test the endpoint before live use.',
    }
  );
}

export function displayModelName(modelId = '') {
  const raw = String(modelId || '').trim();
  if (!raw) return 'Model unavailable';

  const clean = raw
    .replace(/^Argentum llama\.cpp\//i, '')
    .replace(/^llama\.cpp\//i, '')
    .replace(/^local\//i, '');
  const parts = clean.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) || clean || raw;
}

export function inferMimeType(path = '') {
  const lower = String(path || '').toLowerCase();
  if (lower.endsWith('.png')) return 'image/png';
  if (lower.endsWith('.jpg') || lower.endsWith('.jpeg')) return 'image/jpeg';
  if (lower.endsWith('.webp')) return 'image/webp';
  if (lower.endsWith('.gif')) return 'image/gif';
  if (lower.endsWith('.txt') || lower.endsWith('.md')) return 'text/plain';
  if (lower.endsWith('.json')) return 'application/json';
  if (lower.endsWith('.pdf')) return 'application/pdf';
  return 'application/octet-stream';
}

export function inferAttachmentKind(path = '', mime = inferMimeType(path)) {
  if (String(mime).startsWith('image/')) return 'image';
  return 'file';
}

export function filePreviewUrl(path = '') {
  const convertFileSrc = window.__TAURI__?.core?.convertFileSrc;
  if (typeof convertFileSrc === 'function') {
    try {
      return convertFileSrc(path);
    } catch (_error) {
      return '';
    }
  }
  return '';
}

export function modelSupportsVision(modelId, metadata = {}) {
  const capabilities = modelMetadataFor(modelId, metadata).capabilities || [];
  return capabilities.some((capability) => String(capability).toLowerCase().includes('vision'));
}

export function estimateContextTokens(blocks = [], draft = '') {
  const text = [
    ...blocks.map((block) => {
      const attachments = Array.isArray(block.attachments)
        ? block.attachments.map((item) => `${item.name || ''} ${item.mime || ''}`).join(' ')
        : '';
      return `${block.title || ''} ${block.rawBody || block.body || ''} ${block.reasoning || ''} ${attachments}`;
    }),
    draft,
  ].join('\n');
  return Math.max(1, Math.ceil(text.length / 4));
}

function hashText(value = '') {
  let hash = 2166136261;
  const text = String(value);
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16);
}

export function estimateCachedTextTokens(cacheKey, text = '') {
  const source = String(text || '');
  const key = `${cacheKey}:${hashText(source)}`;
  try {
    const storage = typeof window === 'undefined' ? null : window.localStorage;
    const cache = storage
      ? JSON.parse(storage.getItem('argentum.contextTokenCache.v1') || '{}')
      : {};
    if (Number.isFinite(cache[key])) return cache[key];
    const tokens = Math.max(1, Math.ceil(source.length / 4));
    if (storage) {
      storage.setItem(
        'argentum.contextTokenCache.v1',
        JSON.stringify({
          ...cache,
          [key]: tokens,
        }),
      );
    }
    return tokens;
  } catch (_error) {
    return Math.max(1, Math.ceil(source.length / 4));
  }
}

export function defaultCoreContextText(state = {}) {
  const model = String(state.providerModel || '').toLowerCase();
  const contextLimit =
    model.includes('minimax-m2.7') || model.includes('gpt-5.5') || model.includes('gpt-5.4')
      ? 200000
      : model.includes('gemini-2.5')
        ? 1000000
        : 32000;
  const profile = contextLimit >= 1000000 ? 'full' : contextLimit >= 200000 ? 'compact' : 'minimal';
  return [
    `CORE profile: ${profile}`,
    'Local-first desktop AI workspace.',
    'Workspace boundary, approved capabilities, provider usage, gateway state, and user-approved memory guide the agent.',
    'CORE updates require user approval before writes.',
  ].join('\n');
}

export function buildApprovedRuntimeContextText(state) {
  const access = new Set(state.selectedContextAccess || []);
  const lines = [
    `System prompt: ${state.systemPrompt || ''}`,
    `Agent name: ${state.agentName || 'Argentum'}`,
    `User name: ${state.userName || ''}`,
    `Thinking level: ${state.thinkingLevel || 'balanced'}`,
    `Security profile: ${state.securityProfile || 'restricted'}`,
    `App version: ${state.version || 'unknown'}`,
    `Current page: ${state.activeSection || 'chat'}`,
    `View mode: ${state.viewMode || 'chat'}`,
    `Active session: ${state.activeChatId || 'none'}`,
  ];

  if (access.has('workspace-summary')) {
    lines.push(`Workspace: ${state.workspacePath || ''}`);
    lines.push(`Workspace health: ${state.desktopState?.workspaceReady ? 'ready' : 'not ready'}`);
    lines.push(`Gateway PID: ${state.desktopState?.gatewayPid || 'stopped'}`);
  }

  if (access.has('tool-state')) {
    lines.push(
      `Provider status: ${state.apiTest?.status || 'idle'} - ${state.apiTest?.message || ''}`,
    );
    lines.push(`Channels: ${(state.selectedChannels || ['local']).join(', ')}`);
    lines.push(
      `Telegram status: ${state.desktopState?.telegramDiagnostics?.lastResponseStatus || 'not reported'}`,
    );
    if (state.desktopState?.telegramDiagnostics?.lastError) {
      lines.push(`Telegram last error: ${state.desktopState.telegramDiagnostics.lastError}`);
    }
  }

  if (access.has('logs') && state.appLogEntries?.length) {
    lines.push(
      `Recent app log summary: ${state.appLogEntries
        .slice(0, 6)
        .map((entry) => `${entry.event}:${entry.status}`)
        .join(', ')}`,
    );
  }

  if (state.usageSnapshot) {
    const usage = state.usageSnapshot;
    const usageParts = [];
    if (usage.summary) usageParts.push(usage.summary);
    if (usage.requestRemaining) {
      usageParts.push(
        `requests remaining ${usage.requestRemaining}${usage.requestLimit ? ` of ${usage.requestLimit}` : ''}`,
      );
    }
    if (usage.tokenRemaining) {
      usageParts.push(
        `tokens remaining ${usage.tokenRemaining}${usage.tokenLimit ? ` of ${usage.tokenLimit}` : ''}`,
      );
    }
    if (usage.accountUsageStatus) {
      usageParts.push(`account-page usage ${usage.accountUsageStatus}`);
    }
    if (usageParts.length > 0) {
      lines.push(`Provider usage: ${usageParts.join('; ')}`);
    } else {
      lines.push('Provider usage: Usage unavailable from provider');
    }
  }

  lines.push(
    'Argentum app knowledge: visible buttons are fixed app actions; approved model tools may read/write files inside the selected workspace and fetch localhost/loopback URLs. The assistant may summarize approved app state and logs but cannot run arbitrary shell, OS, RAM, browser-session, external-network, or external-folder actions without a permission-gated feature.',
  );

  return lines.filter(Boolean).join('\n');
}

export function estimateRuntimeContextTokens(state, draft = state?.draftMessage || '') {
  const chatTokens = estimateContextTokens(state?.chatBlocks || [], draft);
  const approvedContextTokens = estimateCachedTextTokens(
    'approved-runtime-context',
    buildApprovedRuntimeContextText(state || {}),
  );
  const coreTokens = estimateCachedTextTokens('core-context', defaultCoreContextText(state || {}));
  return Math.max(1, chatTokens + approvedContextTokens + coreTokens);
}

export function compactConversationForProvider(blocks = [], currentMessage = '', options = {}) {
  const maxRecentMessages = options.maxRecentMessages || 14;
  const maxSummaryCharacters = options.maxSummaryCharacters || 1800;
  const messages = blocks
    .filter((block) => block?.type === 'message' && block.body)
    .map((block) => ({
      role: block.role === 'user' ? 'user' : 'assistant',
      content: String(block.body || '').trim(),
    }))
    .filter((message) => message.content.length > 0);

  const recent = messages.slice(-maxRecentMessages);
  const older = messages.slice(0, Math.max(0, messages.length - recent.length));
  const conversationSummary = older
    .map((message) => `${message.role === 'user' ? 'User' : 'Argentum'}: ${message.content}`)
    .join('\n')
    .slice(-maxSummaryCharacters);

  const current = String(currentMessage || '').trim();
  const last = recent.at(-1);
  const conversationHistory =
    current && (!last || last.role !== 'user' || last.content !== current)
      ? [...recent, { role: 'user', content: current }]
      : recent;

  return {
    conversationHistory,
    conversationSummary,
  };
}

export function contextTokenLimit(metadata = {}) {
  if (Number.isFinite(metadata.maxContextTokens)) return metadata.maxContextTokens;

  const label = `${metadata.maxContextWindow || metadata.contextWindow || ''}`.toLowerCase();
  const match = label.match(/([\d.]+)\s*([km])?/);
  if (!match) return 32000;

  const value = Number(match[1]);
  const suffix = match[2];
  if (!Number.isFinite(value) || value <= 0) return 32000;
  if (suffix === 'm') return Math.round(value * 1000000);
  if (suffix === 'k') return Math.round(value * 1000);
  return Math.round(value);
}

export function contextUsagePercent(tokens, metadata = {}) {
  const limit = contextTokenLimit(metadata);
  return Math.max(1, Math.min(100, Math.round((tokens / limit) * 100)));
}

export function renderMarkdown(value) {
  const escaped = escapeHtml(value || '').replace(/\r\n/g, '\n');
  const segments = escaped.split(/(```[\s\S]*?```)/g);

  return segments
    .map((segment) => {
      if (segment.startsWith('```') && segment.endsWith('```')) {
        const code = segment.slice(3, -3).replace(/^\w+\n/, '');
        return `<pre><code>${code}</code></pre>`;
      }

      return segment
        .split(/\n{2,}/)
        .map((paragraph) => renderMarkdownBlock(paragraph))
        .join('');
    })
    .join('');
}

function renderInlineMarkdown(value) {
  return value
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/\*([^*]+)\*/g, '<em>$1</em>')
    .replace(
      /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
      '<a href="$2" data-open-external="$2">$1</a>',
    );
}

function renderMarkdownBlock(block) {
  const lines = block.split('\n').filter((line) => line.trim().length > 0);
  if (lines.length === 0) return '';

  if (lines.every((line) => /^[-*]\s+/.test(line.trim()))) {
    return `<ul>${lines
      .map((line) => `<li>${renderInlineMarkdown(line.trim().replace(/^[-*]\s+/, ''))}</li>`)
      .join('')}</ul>`;
  }

  if (lines.every((line) => /^\d+\.\s+/.test(line.trim()))) {
    return `<ol>${lines
      .map((line) => `<li>${renderInlineMarkdown(line.trim().replace(/^\d+\.\s+/, ''))}</li>`)
      .join('')}</ol>`;
  }

  return `<p>${renderInlineMarkdown(lines.join('<br />'))}</p>`;
}

export function invokeTauri(command, payload) {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) return null;
  return invoke(command, payload);
}

export async function openFolder(defaultPath) {
  const open = window.__TAURI__?.dialog?.open;
  if (!open) return null;

  return open({
    directory: true,
    multiple: false,
    defaultPath,
  });
}

export async function openFile(defaultPath, options = {}) {
  const open = window.__TAURI__?.dialog?.open;
  if (!open) return null;

  return open({
    directory: false,
    multiple: false,
    defaultPath,
    ...(options.filters ? { filters: options.filters } : {}),
  });
}

export function isProbablyAbsolutePath(path) {
  const value = path.trim();
  return /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith('\\\\') || value.startsWith('/');
}

export function explainPath(path) {
  return path && !path.includes('%LOCALAPPDATA%')
    ? path
    : 'the workspace folder you choose in the next step';
}

export function normalizeError(error) {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  return JSON.stringify(error);
}

export function buttonDisabled(disabled) {
  return disabled ? 'disabled' : '';
}
