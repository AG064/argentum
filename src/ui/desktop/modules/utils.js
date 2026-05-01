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
