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
  return modelOptionsFor(provider, '', authMethod).some((option) => option.id === model);
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

export function estimateContextTokens(blocks = [], draft = '') {
  const text = [
    ...blocks.map((block) => `${block.title || ''} ${block.body || ''}`),
    draft,
  ].join('\n');
  return Math.max(1, Math.ceil(text.length / 4));
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

export async function openFile(defaultPath) {
  const open = window.__TAURI__?.dialog?.open;
  if (!open) return null;

  return open({
    directory: false,
    multiple: false,
    defaultPath,
  });
}

export function isProbablyAbsolutePath(path) {
  const value = path.trim();
  return /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith('\\\\') || value.startsWith('/');
}

export function explainPath(path) {
  return path && !path.includes('%LOCALAPPDATA%') ? path : 'the workspace folder you choose in the next step';
}

export function normalizeError(error) {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  return JSON.stringify(error);
}

export function buttonDisabled(disabled) {
  return disabled ? 'disabled' : '';
}
