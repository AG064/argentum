import { APP_VERSION, providerPresets } from './constants.js';
import { ensureProviderModelAllowed, notify, state } from './state.js';
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
    providerAuthMethod: state.providerAuthMethod || 'api-key',
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

export async function persistRuntimeSettings(reason = 'settings', options = {}) {
  if (!state.setupComplete) return null;

  ensureProviderModelAllowed();
  const result = await saveSetup();
  state.savedConfigPath = result?.configPath || result?.config_path || state.savedConfigPath;

  if (options.notify) {
    notify('success', 'Settings saved', `Runtime settings were saved for ${reason}.`);
  }

  return result;
}

export async function testProvider() {
  const provider = currentProvider(providerPresets, state);
  ensureProviderModelAllowed();
  state.apiTest = {
    status: 'testing',
    message: `Testing ${provider.label}...`,
  };

  if (state.setupComplete) {
    try {
      await persistRuntimeSettings('provider-test');
    } catch (error) {
      state.apiTest = {
        status: 'error',
        message: normalizeError(error),
      };
      notify('error', 'Settings could not be saved', state.apiTest.message);
      return state.apiTest;
    }
  }

  const request = {
    provider: state.llmProvider,
    api: state.providerApi || provider.api,
    baseUrl: state.providerBaseUrl || provider.defaultBaseUrl,
    apiKey: state.providerApiKey,
    model: state.providerModel || provider.defaultModel,
    authMethod: state.providerAuthMethod || 'api-key',
    workspacePath: state.workspacePath,
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

export async function startCodexOAuth() {
  state.codexOAuth = {
    ...state.codexOAuth,
    status: 'starting',
    message: 'Requesting an OpenAI/Codex authorization code...',
  };

  const promise = invokeTauri('start_codex_oauth', {
    request: {
      workspacePath: state.workspacePath,
    },
  });

  if (!promise) {
    state.codexOAuth = {
      ...state.codexOAuth,
      status: 'preview',
      message: 'Preview mode: run the installed Argentum app to start OpenAI/Codex authorization.',
      verificationUrl: 'https://auth.openai.com/codex/device',
      userCode: 'PREVIEW',
      deviceAuthId: '',
      interval: 5,
      codexHome: `${state.workspacePath}\\data\\codex-oauth`,
    };
    notify('warning', 'Authorization preview', state.codexOAuth.message);
    return state.codexOAuth;
  }

  try {
    const result = await promise;
    state.codexOAuth = {
      status: result.status || 'pending',
      message: result.message || 'Open the verification URL and enter the code.',
      verificationUrl: result.verificationUrl || '',
      userCode: result.userCode || '',
      deviceAuthId: result.deviceAuthId || '',
      interval: result.interval || 5,
      codexHome: result.codexHome || '',
    };
    notify('info', 'OpenAI/Codex authorization', state.codexOAuth.message);
    return state.codexOAuth;
  } catch (error) {
    state.codexOAuth = {
      ...state.codexOAuth,
      status: 'error',
      message: normalizeError(error),
    };
    notify('error', 'Authorization failed', state.codexOAuth.message);
    return state.codexOAuth;
  }
}

export async function completeCodexOAuth() {
  if (!state.codexOAuth.deviceAuthId || !state.codexOAuth.userCode) {
    state.codexOAuth = {
      ...state.codexOAuth,
      status: 'error',
      message: 'Start OpenAI/Codex authorization first, then complete it after approving in the browser.',
    };
    notify('error', 'Authorization needs a code', state.codexOAuth.message);
    return state.codexOAuth;
  }

  state.codexOAuth = {
    ...state.codexOAuth,
    status: 'completing',
    message: 'Checking whether OpenAI/Codex authorization is complete...',
  };

  const promise = invokeTauri('complete_codex_oauth', {
    request: {
      workspacePath: state.workspacePath,
      deviceAuthId: state.codexOAuth.deviceAuthId,
      userCode: state.codexOAuth.userCode,
      interval: state.codexOAuth.interval,
    },
  });

  if (!promise) {
    state.codexOAuth = {
      ...state.codexOAuth,
      status: 'preview',
      message: 'Preview mode: install or run the Tauri app to finish OpenAI/Codex authorization.',
    };
    notify('warning', 'Authorization preview', state.codexOAuth.message);
    return state.codexOAuth;
  }

  try {
    const result = await promise;
    state.codexOAuth = {
      ...state.codexOAuth,
      status: result.status || 'ok',
      message: result.message || 'OpenAI/Codex authorization saved.',
      codexHome: result.codexHome || state.codexOAuth.codexHome,
    };

    if (result.status === 'ok') {
      state.providerAuthMethod = 'browser-account';
      state.providerApiKey = '';
      ensureProviderModelAllowed();
      state.providerSetupStage = 'model';
      state.providerSelectionConfirmed = true;
      state.apiTest = {
        status: 'idle',
        message: 'OpenAI/Codex authorization is saved. Run Test Provider to verify the saved browser-account credentials.',
      };
      try {
        const saveResult = await saveSetup();
        state.savedConfigPath = saveResult.configPath || saveResult.config_path || state.savedConfigPath;
      } catch (error) {
        notify('warning', 'Authorization saved, config not updated', normalizeError(error));
      }
      notify('success', 'Authorization saved', state.codexOAuth.message);
    } else {
      notify('warning', 'Authorization pending', state.codexOAuth.message);
    }

    return state.codexOAuth;
  } catch (error) {
    state.codexOAuth = {
      ...state.codexOAuth,
      status: 'error',
      message: normalizeError(error),
    };
    notify('error', 'Authorization failed', state.codexOAuth.message);
    return state.codexOAuth;
  }
}

export async function openExternalUrl(url) {
  const target = String(url || '').trim();
  if (!target) return false;

  const promise = invokeTauri('open_external_url', {
    request: {
      url: target,
    },
  });

  if (!promise) {
    window.open(target, '_blank', 'noopener,noreferrer');
    return true;
  }

  try {
    await promise;
    return true;
  } catch (error) {
    notify('error', 'Could not open browser', normalizeError(error));
    return false;
  }
}

export async function sendChatMessage(message) {
  await persistRuntimeSettings('chat');
  const promise = invokeTauri('send_chat_message', {
    request: {
      workspacePath: state.workspacePath,
      message,
    },
  });

  if (!promise) {
    return {
      status: 'offline',
      message:
        'Desktop preview mode: install or run the Tauri app to send live provider messages. Local setup help remains available here.',
      provider: 'Preview',
      model: 'local-guided',
      offline: true,
    };
  }

  return promise;
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
