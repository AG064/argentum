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
