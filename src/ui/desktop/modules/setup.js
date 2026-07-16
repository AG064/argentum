import { APP_VERSION, providerPresets } from './constants.js';
import {
  ensureProviderModelAllowed,
  mergeChannelChatSessions,
  notify,
  recordUiEvent,
  setUiPreference,
  state,
} from './state.js';
import {
  compactConversationForProvider,
  currentProvider,
  invokeTauri,
  normalizeError,
  openFolder,
} from './utils.js';

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
    agentName: state.agentName,
    userName: state.userName,
    systemPrompt: state.systemPrompt,
    selectedContextAccess: state.selectedContextAccess,
    thinkingLevel: state.thinkingLevel,
    showThinkingInChat: state.showThinkingInChat,
    showThinkingInTelegram: state.showThinkingInTelegram,
    selectedChannels: state.selectedChannels,
    webchatToken: state.webchatToken,
    telegramToken: state.telegramToken,
    telegramAllowlist: state.telegramAllowlist,
    whatsappPhoneId: state.whatsappPhoneId,
    securityProfile: state.securityProfile,
    version: APP_VERSION,
  };
}

export function providerUsageLine(usage) {
  if (!usage) return 'Usage is unavailable until a provider test or chat response returns limits.';

  const windows = Array.isArray(usage.modalityQuotas) ? usage.modalityQuotas : [];
  const windowLine = windows
    .map((window) => {
      const remaining = window.remaining || 'unknown remaining';
      const limit = window.limit ? ` of ${window.limit}` : '';
      const cadence = window.resetCadence || window.reset || 'reset not reported';
      return `${window.label}: ${remaining}${limit}, ${cadence}`;
    })
    .join(' | ');

  const requestLine = usage.fiveHourRequestLimit
    ? `M2.7 rolling window: ${usage.requestRemaining || 'unknown remaining'} of ${usage.fiveHourRequestLimit}`
    : usage.requestRemaining
      ? `Requests remaining: ${usage.requestRemaining}`
      : '';
  const weeklyLine = usage.weeklyRequestBudget
    ? `Weekly budget overlay: ${usage.weeklyRequestBudget}`
    : '';
  const accountLine = usage.accountUsageStatus ? `Account page: ${usage.accountUsageStatus}` : '';
  const summary =
    usage.summary || [requestLine, weeklyLine, accountLine, windowLine].filter(Boolean).join(' | ');

  return summary || 'Usage unavailable from provider';
}

function previewSystemStats() {
  const memory = window.navigator?.deviceMemory
    ? Number(window.navigator.deviceMemory) * 1024 ** 3
    : 0;
  const cores = Number(window.navigator?.hardwareConcurrency || 0);
  return {
    collectedAt: String(Math.floor(Date.now() / 1000)),
    hostName: 'Preview browser',
    osName: 'Desktop preview',
    osVersion: 'Unavailable until the installed app runs',
    kernelVersion: 'Unavailable',
    arch: window.navigator?.platform || 'Browser preview',
    cpuBrand: cores
      ? `${cores} logical cores reported by browser`
      : 'Unavailable in browser preview',
    cpuCores: cores,
    cpuCoresDetail: Array.from({ length: Math.max(cores, 0) }, (_, index) => ({
      core: index,
      name: `Browser core ${index + 1}`,
      frequencyMhz: 0,
      usagePercent: 0,
    })),
    cpuUsagePercent: 0,
    memoryTotalBytes: memory,
    memoryUsedBytes: 0,
    memoryUsedPercent: 0,
    swapTotalBytes: 0,
    swapUsedBytes: 0,
    diskTotalBytes: 0,
    diskAvailableBytes: 0,
    diskUsedPercent: 0,
    networkReceivedBytes: 0,
    networkTransmittedBytes: 0,
    uptimeSeconds: 0,
    temperatureCelsius: null,
    processesCount: 0,
    processes: [],
    networks: [],
    temperatureSensors: [],
    gpus: [],
    disks: [],
  };
}

export async function saveSetup() {
  const request = buildSetupPayload();
  const promise = invokeTauri('save_setup', { request });

  if (!promise) {
    recordUiEvent('onboarding.save', 'preview', 'Preview setup saved locally in browser state.', {
      workspacePath: request.workspacePath,
    });
    return {
      status: 'setup_saved',
      configPath: `${request.workspacePath}\\config\\default.yaml`,
      secretsPath: `${request.workspacePath}\\secrets.env`,
    };
  }

  const result = await promise;
  recordUiEvent('onboarding.save', 'ok', 'Configuration was saved to the selected workspace.', {
    workspacePath: request.workspacePath,
  });
  return result;
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
  recordUiEvent('provider.test', 'running', state.apiTest.message, {
    provider: provider.id,
    authMethod: state.providerAuthMethod || 'api-key',
    model: state.providerModel || provider.defaultModel,
  });

  if (state.setupComplete) {
    try {
      await persistRuntimeSettings('provider-test');
    } catch (error) {
      state.apiTest = {
        status: 'error',
        message: normalizeError(error),
      };
      notify('error', 'Settings could not be saved', state.apiTest.message);
      recordUiEvent('provider.test', 'error', state.apiTest.message, {
        provider: provider.id,
      });
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
    const localPreview =
      request.baseUrl.includes('127.0.0.1') || request.baseUrl.includes('localhost');
    state.apiTest = {
      status: localPreview || request.apiKey ? 'ok' : 'warning',
      message: localPreview
        ? 'Preview mode: local endpoint shape looks ready.'
        : 'Preview mode: add an API key in the installed app to run a live test.',
    };
    notify(
      state.apiTest.status === 'ok' ? 'success' : 'warning',
      'Provider test',
      state.apiTest.message,
    );
    recordUiEvent('provider.test', state.apiTest.status, providerUsageLine(state.usageSnapshot), {
      provider: provider.id,
      preview: true,
    });
    return state.apiTest;
  }

  try {
    const result = await promise;
    state.apiTest = {
      status: result.status === 'ok' ? 'ok' : 'warning',
      message: result.message || 'Provider test completed.',
    };
    if (result.usage) state.usageSnapshot = result.usage;
    notify(
      state.apiTest.status === 'ok' ? 'success' : 'warning',
      'Provider test',
      state.apiTest.message,
    );
    recordUiEvent('provider.test', state.apiTest.status, providerUsageLine(result.usage), {
      provider: provider.id,
      model: request.model,
    });
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
    recordUiEvent('provider.test', 'error', state.apiTest.message, {
      provider: provider.id,
      model: request.model,
    });
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
      message:
        'Start OpenAI/Codex authorization first, then complete it after approving in the browser.',
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
        message:
          'OpenAI/Codex authorization is saved. Run Test Provider to verify the saved browser-account credentials.',
      };
      try {
        const saveResult = await saveSetup();
        state.savedConfigPath =
          saveResult.configPath || saveResult.config_path || state.savedConfigPath;
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

export async function buildChatRequestPayload(message, attachments = []) {
  await persistRuntimeSettings('chat');
  const { conversationHistory, conversationSummary } = compactConversationForProvider(
    state.chatBlocks,
    message,
  );
  return {
    workspacePath: state.workspacePath,
    message,
    agentName: state.agentName,
    userName: state.userName,
    systemPrompt: state.systemPrompt,
    selectedContextAccess: state.selectedContextAccess,
    thinkingLevel: state.thinkingLevel,
    securityProfile: state.securityProfile,
    selectedChannels: state.selectedChannels,
    conversationHistory,
    conversationSummary,
    attachments,
  };
}

export async function sendChatMessage(message, attachments = []) {
  const request = await buildChatRequestPayload(message, attachments);
  recordUiEvent('chat.send', 'running', 'Sending chat message to the configured provider.', {
    provider: state.llmProvider,
    model: state.providerModel,
    historyMessages: request.conversationHistory.length,
  });
  const promise = invokeTauri('send_chat_message', {
    request,
  });

  if (!promise) {
    recordUiEvent(
      'chat.send',
      'offline',
      'Desktop bridge unavailable; chat stayed in preview mode.',
      {
        provider: 'Preview',
      },
    );
    return {
      status: 'offline',
      message:
        'Desktop preview mode: install or run the Tauri app to send live provider messages. Local setup help remains available here.',
      provider: 'Preview',
      model: 'local-guided',
      offline: true,
    };
  }

  const result = await promise;
  if (result.usage) state.usageSnapshot = result.usage;
  recordUiEvent(
    'chat.send',
    result.offline ? 'offline' : result.status || 'ok',
    result.message || 'Chat response received.',
    {
      provider: result.provider,
      model: result.model,
      usage: providerUsageLine(result.usage),
    },
  );
  return result;
}

export async function chooseWorkspaceFolder() {
  const selected = await openFolder(state.workspacePath);
  if (!selected) {
    notify('info', 'Folder not changed', 'No workspace folder was selected.');
    return false;
  }

  state.workspacePath = Array.isArray(selected) ? selected[0] : selected;
  setUiPreference('workspacePath', state.workspacePath);
  detectMigrationSources(); // Check for OpenClaw/Hermes to migrate
  notify(
    'success',
    'Workspace selected',
    `Default access is now limited to files and folders inside ${state.workspacePath}.`,
  );
  return true;
}

/** Detect OpenClaw and Hermes installation sources for migration. */
export async function detectMigrationSources() {
  if (typeof window === 'undefined' || !window.__TAURI__) return;
  try {
    const { invoke } = window.__TAURI__.core;
    const sources = await invoke('detect_migration_sources');
    state.migrationSources = sources;
    // OpenClaw is the priority source for v0.0.9
    state.migrationDetected = Boolean(sources?.openclaw?.found || sources?.hermes?.found);
    state.migrationSkipped = false;
    state.migrationResults = null;
    state.migrationError = '';
  } catch (err) {
    console.warn('[Migration] Detection failed:', err);
    state.migrationSources = { openclaw: null, hermes: null };
    state.migrationDetected = false;
  }
}

/** Migrate selected items from OpenClaw into the current workspace. */
export async function runMigration(items) {
  if (!state.workspacePath) throw new Error('No workspace selected');
  state.migrationInProgress = true;
  state.migrationError = '';
  try {
    const { invoke } = window.__TAURI__.core;
    const results = await invoke('migrate_from_openclaw', {
      workspacePath: state.workspacePath,
      items,
    });
    state.migrationResults = results;
    return results;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    state.migrationError = msg;
    throw new Error(msg);
  } finally {
    state.migrationInProgress = false;
  }
}

/** Handle the "Import" button click in the onboarding migration card. */
export async function handleMigrationImport() {
  const openclaw = state.migrationSources?.openclaw;
  const items = (openclaw?.items || []).map((item) => ({
    id: item.id,
    source_path: item.source_path,
  }));

  if (!items.length) return;

  try {
    await runMigration(items);
    notify('success', 'Migration complete', `${items.length} item(s) imported from OpenClaw.`);
  } catch (err) {
    notify('warning', 'Migration partial', err.message || 'Some items may not have migrated.');
  }
}

export async function hydrateDesktopDefaults() {
  const promise = invokeTauri('desktop_defaults');
  if (!promise) return;

  try {
    const defaults = await promise;
    if (defaults?.savedWorkspacePath) {
      state.workspacePath = defaults.savedWorkspacePath;
      setUiPreference('workspacePath', state.workspacePath);
      detectMigrationSources();
      return;
    }
    if (
      defaults?.defaultWorkspacePath &&
      (!state.workspacePath || state.workspacePath.includes('%LOCALAPPDATA%'))
    ) {
      state.workspacePath = defaults.defaultWorkspacePath;
      setUiPreference('workspacePath', state.workspacePath);
      detectMigrationSources();
    }
  } catch (error) {
    notify('warning', 'Default workspace unavailable', normalizeError(error));
  }
}

export async function refreshDesktopState(options = {}) {
  const { announce = false, silentErrors = false } = options;
  const promise = invokeTauri('desktop_state', {
    request: {
      workspacePath: state.workspacePath,
      llamaServer: state.llamaServerConfig,
    },
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
      llamaServerInstalled: false,
      llamaServerPid: null,
      llamaServerEndpoint: `http://${state.llamaServerConfig.host || '127.0.0.1'}:${state.llamaServerConfig.port || 8080}/v1`,
      llamaServerLogPreview:
        'Desktop preview mode. Run the installed app to read local server logs.',
      gatewayLogPreview: 'Desktop preview mode. Run the installed app to read local logs.',
      auditLogPreview: 'Desktop preview mode. Run the installed app to read audit history.',
      appLogPreview:
        'Desktop preview mode. Run the installed app to read structured activity logs.',
      systemStats: previewSystemStats(),
      telegramDiagnostics: {
        configured: state.selectedChannels.includes('telegram'),
        lastResponseStatus: state.selectedChannels.includes('telegram')
          ? 'preview-mode'
          : 'not-selected',
      },
    };
    if (announce) notify('success', 'Workspace state refreshed', 'Preview state was refreshed.');
    return;
  }

  try {
    const result = await promise;
    state.desktopState = result;
    mergeChannelChatSessions(result.channelSessions || []);
    if (announce)
      notify('success', 'Workspace state refreshed', 'Local workspace state was refreshed.');
  } catch (error) {
    if (!silentErrors) notify('error', 'Workspace state failed', normalizeError(error));
  }
}

export async function refreshSystemDashboardState(options = {}) {
  const { silentErrors = false } = options;
  const promise = invokeTauri('desktop_system_stats');

  if (!promise) {
    state.desktopState = {
      ...state.desktopState,
      systemStats: previewSystemStats(),
    };
    return state.desktopState.systemStats;
  }

  try {
    const result = await promise;
    state.desktopState = {
      ...state.desktopState,
      systemStats: result,
    };
    return result;
  } catch (error) {
    if (!silentErrors) notify('error', 'System dashboard failed', normalizeError(error));
    return null;
  }
}
