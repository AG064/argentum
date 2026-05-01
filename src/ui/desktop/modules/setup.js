import { APP_VERSION, providerPresets } from './constants.js';
import { notify, state } from './state.js';
import { currentProvider, invokeTauri, normalizeError, openFolder } from './utils.js';

export function buildSetupPayload() {
  const provider = currentProvider(providerPresets, state);
  return {
    workspacePath: state.workspacePath,
    experienceLevel: state.experienceLevel,
    runtimeMode: state.runtimeMode,
    llmProvider: state.llmProvider,
    providerApi: state.providerApi || provider.api,
    providerBaseUrl: state.providerBaseUrl || provider.defaultBaseUrl,
    providerModel: state.providerModel || provider.defaultModel,
    providerApiKey: state.providerApiKey,
    providerApiKeyEnv: state.customApiKeyEnv || provider.apiKeyEnv,
    customProviderName: state.customProviderName,
    selectedChannels: state.selectedChannels,
    webchatToken: state.webchatToken,
    telegramToken: state.telegramToken,
    telegramAllowlist: state.telegramAllowlist,
    whatsappPhoneId: state.whatsappPhoneId,
    securityProfile: state.securityProfile,
    version: APP_VERSION,
  };
}

export async function saveSetup() {
  const request = buildSetupPayload();
  const promise = invokeTauri('save_setup', { request });

  if (!promise) {
    return {
      status: 'setup_saved',
      configPath: `${request.workspacePath}\\config\\default.yaml`,
      secretsPath: `${request.workspacePath}\\secrets.env`,
    };
  }

  return promise;
}

export async function testProvider() {
  const provider = currentProvider(providerPresets, state);
  state.apiTest = {
    status: 'testing',
    message: `Testing ${provider.label}...`,
  };

  const request = {
    provider: state.llmProvider,
    api: state.providerApi || provider.api,
    baseUrl: state.providerBaseUrl || provider.defaultBaseUrl,
    apiKey: state.providerApiKey,
    model: state.providerModel || provider.defaultModel,
  };

  const promise = invokeTauri('test_provider', { request });
  if (!promise) {
    const localPreview = request.baseUrl.includes('127.0.0.1') || request.baseUrl.includes('localhost');
    state.apiTest = {
      status: localPreview || request.apiKey ? 'ok' : 'warning',
      message: localPreview
        ? 'Preview mode: local endpoint shape looks ready.'
        : 'Preview mode: add an API key in the installed app to run a live test.',
    };
    notify(state.apiTest.status === 'ok' ? 'success' : 'warning', 'Provider test', state.apiTest.message);
    return state.apiTest;
  }

  try {
    const result = await promise;
    state.apiTest = {
      status: result.status === 'ok' ? 'ok' : 'warning',
      message: result.message || 'Provider test completed.',
    };
    notify(state.apiTest.status === 'ok' ? 'success' : 'warning', 'Provider test', state.apiTest.message);
    return state.apiTest;
  } catch (error) {
    state.apiTest = {
      status: 'error',
      message: normalizeError(error),
    };
    notify(
      'error',
      'Provider test failed',
      `${state.apiTest.message} Check the provider, key, endpoint, and model, then test again.`,
    );
    return state.apiTest;
  }
}

export async function chooseWorkspaceFolder() {
  const selected = await openFolder(state.workspacePath);
  if (!selected) {
    notify('info', 'Folder not changed', 'No workspace folder was selected.');
    return false;
  }

  state.workspacePath = Array.isArray(selected) ? selected[0] : selected;
  notify(
    'success',
    'Workspace selected',
    `Default access is now limited to files and folders inside ${state.workspacePath}.`,
  );
  return true;
}

export async function hydrateDesktopDefaults() {
  const promise = invokeTauri('desktop_defaults');
  if (!promise) return;

  try {
    const defaults = await promise;
    if (defaults?.defaultWorkspacePath && state.workspacePath.includes('%LOCALAPPDATA%')) {
      state.workspacePath = defaults.defaultWorkspacePath;
    }
  } catch (error) {
    notify('warning', 'Default workspace unavailable', normalizeError(error));
  }
}

export async function refreshDesktopState(options = {}) {
  const { announce = false } = options;
  const promise = invokeTauri('desktop_state', {
    request: { workspacePath: state.workspacePath },
  });

  if (!promise) {
    state.desktopState = {
      workspacePath: state.workspacePath,
      configPath: `${state.workspacePath}\\config\\default.yaml`,
      workspaceReady: Boolean(state.setupComplete),
      configExists: Boolean(state.setupComplete),
      dataExists: Boolean(state.setupComplete),
      logsExists: Boolean(state.setupComplete),
      gatewayPid: null,
      gatewayLogPreview: 'Desktop preview mode. Run the installed app to read local logs.',
      auditLogPreview: 'Desktop preview mode. Run the installed app to read audit history.',
    };
    if (announce) notify('success', 'Workspace state refreshed', 'Preview state was refreshed.');
    return;
  }

  try {
    state.desktopState = await promise;
    if (announce) notify('success', 'Workspace state refreshed', 'Local workspace state was refreshed.');
  } catch (error) {
    notify('error', 'Workspace state failed', normalizeError(error));
  }
}
