import { APP_VERSION, fontOptions, providerPresets } from './constants.js';
import { defaultModelForAuth, modelAllowedForAuth } from './utils.js';

const defaultProvider = providerPresets.find((provider) => provider.id === 'openai');
const CHAT_HISTORY_STORAGE_KEY = 'argentum.chatHistory.v1';
const UI_PREFERENCES_STORAGE_KEY = 'argentum.uiPreferences.v1';

const openingChatBlocks = [
  {
    type: 'message',
    role: 'argentum',
    title: 'Argentum',
    body: 'I am ready. Finish onboarding when it appears, then we can tune your profile, provider, and workspace permissions from here.',
  },
];

function cloneBlocks(blocks) {
  return JSON.parse(JSON.stringify(blocks));
}

function summarizeChat(blocks) {
  const lastMessage = [...blocks]
    .reverse()
    .find((block) => block.type === 'message' && block.body);
  if (!lastMessage) return 'No messages yet';

  const text = String(lastMessage.body).replace(/\s+/g, ' ').trim();
  return text.length > 68 ? `${text.slice(0, 65)}...` : text;
}

function titleFromChat(blocks, fallback) {
  const firstUserMessage = blocks.find((block) => block.type === 'message' && block.role === 'user');
  if (!firstUserMessage?.body) return fallback;

  const text = String(firstUserMessage.body).replace(/\s+/g, ' ').trim();
  return text.length > 34 ? `${text.slice(0, 31)}...` : text;
}

function persistChatHistory() {
  try {
    const storage = typeof window === 'undefined' ? null : window.localStorage;
    if (!storage) return;
    storage.setItem(
      CHAT_HISTORY_STORAGE_KEY,
      JSON.stringify({
        activeChatId: state.activeChatId,
        chatSessions: state.chatSessions,
      }),
    );
  } catch (_error) {
    // Chat history persistence should never prevent the app from opening.
  }
}

export const state = {
  activeSection: 'chat',
  onboardingStep: 1,
  onboardingOpen: true,
  setupComplete: false,
  setupStatus: 'setup_pending',
  setupAnimation: false,
  version: APP_VERSION,
  workspacePath: '%LOCALAPPDATA%\\Programs\\Argentum\\workspace',
  experienceLevel: 'beginner',
  runtimeMode: 'desktop',
  llmProvider: 'openai',
  providerApi: defaultProvider.api,
  providerBaseUrl: defaultProvider.defaultBaseUrl,
  providerModel: defaultProvider.defaultModel,
  providerAuthMethod: 'api-key',
  providerCatalogTab: 'stable',
  providerSetupStage: 'provider',
  providerSelectionConfirmed: false,
  providerApiKey: '',
  codexOAuth: {
    status: 'idle',
    message: 'OpenAI/Codex authorization has not been started.',
    verificationUrl: '',
    userCode: '',
    deviceAuthId: '',
    interval: 5,
    codexHome: '',
  },
  customProviderName: 'custom',
  customApiKeyEnv: 'CUSTOM_API_KEY',
  selectedChannels: ['local'],
  webchatToken: '',
  telegramToken: '',
  telegramAllowlist: '',
  whatsappPhoneId: '',
  securityProfile: 'restricted',
  apiTest: {
    status: 'idle',
    message: 'Provider has not been tested yet.',
  },
  usageSnapshot: null,
  notifications: [
    {
      id: 'welcome',
      type: 'info',
      title: 'Welcome',
      message: 'Choose a workspace folder first. Argentum will keep default access inside that folder.',
    },
  ],
  notificationHistory: [
    {
      id: 'welcome',
      type: 'info',
      title: 'Welcome',
      message: 'Choose a workspace folder first. Argentum will keep default access inside that folder.',
    },
  ],
  notificationsMuted: false,
  notificationsMenuOpen: false,
  uiFontFamily: fontOptions.ui[0].css,
  codeFontFamily: fontOptions.mono[0].css,
  savedConfigPath: '',
  actionStatus: 'No GUI action has run in this session.',
  runningAction: '',
  copiedCommand: '',
  userName: '',
  agentName: 'Argentum',
  systemPrompt:
    'You are Argentum, a secure desktop AI agent. Be direct, practical, and stay within the selected workspace and approved capabilities.',
  agentPurpose: '',
  thinkingLevel: 'balanced',
  chatStreaming: false,
  chatAttachments: [],
  voiceInputStatus: 'idle',
  selectedContextAccess: ['workspace-summary', 'profile', 'tool-state'],
  activeChatId: 'setup',
  pendingDeleteChatId: '',
  chatSessions: [
    {
      id: 'setup',
      title: 'Setup and security',
      subtitle: 'Workspace, provider, and permissions',
      blocks: cloneBlocks(openingChatBlocks),
      updatedAt: Date.now(),
    },
    {
      id: 'general',
      title: 'General chat',
      subtitle: 'Ask Argentum directly',
      blocks: [
        {
          type: 'message',
          role: 'argentum',
          title: 'Argentum',
          body: 'Start a fresh conversation here. I will keep this thread separate from setup history.',
        },
      ],
      updatedAt: Date.now() - 1,
    },
  ],
  desktopState: {
    workspaceReady: false,
    configExists: false,
    dataExists: false,
    logsExists: false,
    gatewayPid: null,
    gatewayLogPreview: 'No entries yet.',
    auditLogPreview: 'No entries yet.',
  },
  chatBlocks: cloneBlocks(openingChatBlocks),
  draftMessage: '',
  terminalEntries: [
    {
      id: 'terminal-boot',
      status: 'info',
      command: 'argentum desktop',
      output: 'Desktop shell loaded. Gateway, chat, diagnostics, and setup output will appear here when actions run.',
    },
  ],
  pendingApprovals: [
    {
      id: 'workspace-read',
      title: 'Read selected workspace',
      detail: 'Allowed only for files and folders under the workspace path shown in the rail.',
      status: 'Allowed by default',
    },
    {
      id: 'shell-run',
      title: 'Run shell command',
      detail: 'Requires command, working directory, reason, and expiration before execution.',
      status: 'Ask every time',
    },
    {
      id: 'network-send',
      title: 'External network request',
      detail: 'Requires destination, provider, and purpose before it can leave your machine.',
      status: 'Blocked',
    },
  ],
};

export function notify(type, title, message) {
  const id = `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const notification = { id, type, title, message };
  state.notificationHistory = [notification, ...state.notificationHistory].slice(0, 40);

  if (!state.notificationsMuted) {
    state.notifications = [notification, ...state.notifications].slice(0, 4);
    if (typeof window !== 'undefined') {
      window.setTimeout(() => {
        dismissNotification(id, { silent: true });
        window.dispatchEvent(new Event('argentum:state-change'));
      }, 5200);
    }
  }

  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event('argentum:state-change'));
  }

  return id;
}

export function dismissNotification(id, options = {}) {
  state.notifications = state.notifications.filter((notification) => notification.id !== id);
  if (!options.silent && typeof window !== 'undefined') {
    window.dispatchEvent(new Event('argentum:state-change'));
  }
}

export function clearNotifications() {
  state.notifications = [];
  state.notificationHistory = [];
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event('argentum:state-change'));
  }
}

export function scheduleVisibleNotifications() {
  if (typeof window === 'undefined') return;

  for (const notification of state.notifications) {
    window.setTimeout(() => {
      dismissNotification(notification.id, { silent: true });
      window.dispatchEvent(new Event('argentum:state-change'));
    }, 5200);
  }
}

export function toggleNotificationsMuted() {
  state.notificationsMuted = !state.notificationsMuted;
  if (state.notificationsMuted) state.notifications = [];
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event('argentum:state-change'));
  }
}

export function toggleNotificationsMenu() {
  state.notificationsMenuOpen = !state.notificationsMenuOpen;
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event('argentum:state-change'));
  }
}

export function addTerminalEntry(command, output, status = 'info') {
  state.terminalEntries = [
    {
      id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
      status,
      command,
      output,
    },
    ...state.terminalEntries,
  ].slice(0, 24);
}

export function terminalEntriesForDisplay(filter = '') {
  const entries = filter
    ? state.terminalEntries.filter((entry) => entry.command.includes(filter))
    : state.terminalEntries;

  return entries.slice(0, 8).reverse();
}

function sortChatSessions(sessions) {
  return [...sessions]
    .sort((a, b) => {
      if (a.id === state.activeChatId) return -1;
      if (b.id === state.activeChatId) return 1;
      return (b.updatedAt || 0) - (a.updatedAt || 0);
    })
    .slice(0, 24);
}

export function touchActiveChatSession() {
  const index = state.chatSessions.findIndex((chat) => chat.id === state.activeChatId);
  if (index !== -1) {
    state.chatSessions[index] = {
      ...state.chatSessions[index],
      updatedAt: Date.now(),
    };
  }
  state.chatSessions = sortChatSessions(state.chatSessions);
  persistChatHistory();
}

export function syncActiveChatSession() {
  const index = state.chatSessions.findIndex((chat) => chat.id === state.activeChatId);
  if (index === -1) return;

  const current = state.chatSessions[index];
  const fallbackTitle = current.title || 'New chat';
  state.chatSessions[index] = {
    ...current,
    title: titleFromChat(state.chatBlocks, fallbackTitle === 'New chat' ? 'New chat' : fallbackTitle),
    subtitle: summarizeChat(state.chatBlocks),
    blocks: cloneBlocks(state.chatBlocks),
    updatedAt: Date.now(),
  };
  state.chatSessions = sortChatSessions(state.chatSessions);
  persistChatHistory();
}

export function setActiveChatSession(chatId) {
  syncActiveChatSession();
  const session = state.chatSessions.find((chat) => chat.id === chatId) || state.chatSessions[0];
  if (!session) return;

  state.activeChatId = session.id;
  state.chatBlocks = cloneBlocks(session.blocks?.length ? session.blocks : openingChatBlocks);
  state.draftMessage = '';
  state.chatAttachments = [];
  touchActiveChatSession();
}

export function createChatSession() {
  syncActiveChatSession();
  const id = `chat-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const session = {
    id,
    title: 'New chat',
    subtitle: 'No messages yet',
    blocks: [
      {
        type: 'message',
        role: 'argentum',
        title: state.agentName || 'Argentum',
        body: 'New chat started. Ask a question, attach a workspace file, or describe what you want Argentum to do.',
      },
    ],
    updatedAt: Date.now(),
  };

  state.chatSessions = [session, ...state.chatSessions].slice(0, 24);
  state.activeChatId = id;
  state.chatBlocks = cloneBlocks(session.blocks);
  state.draftMessage = '';
  state.chatAttachments = [];
  persistChatHistory();
  return session;
}

export function requestDeleteChatSession(chatId) {
  state.pendingDeleteChatId = chatId;
}

export function cancelDeleteChatSession() {
  state.pendingDeleteChatId = '';
}

export function confirmDeleteChatSession(chatId) {
  syncActiveChatSession();
  const remaining = state.chatSessions.filter((chat) => chat.id !== chatId);

  if (remaining.length === 0) {
    const replacement = {
      id: 'general',
      title: 'General chat',
      subtitle: 'No messages yet',
      blocks: cloneBlocks(openingChatBlocks),
      updatedAt: Date.now(),
    };
    state.chatSessions = [replacement];
  } else {
    state.chatSessions = remaining;
  }

  state.pendingDeleteChatId = '';
  if (!state.chatSessions.some((chat) => chat.id === state.activeChatId)) {
    state.activeChatId = state.chatSessions[0].id;
    state.chatBlocks = cloneBlocks(state.chatSessions[0].blocks?.length ? state.chatSessions[0].blocks : openingChatBlocks);
  }

  state.chatSessions = sortChatSessions(state.chatSessions);
  persistChatHistory();
}

export function setProviderCatalogTab(tabId) {
  state.providerCatalogTab = tabId === 'beta' ? 'beta' : 'stable';
}

export function hydrateChatHistory() {
  try {
    const storage = typeof window === 'undefined' ? null : window.localStorage;
    if (!storage) return false;
    const saved = JSON.parse(storage.getItem(CHAT_HISTORY_STORAGE_KEY) || 'null');
    if (!saved || !Array.isArray(saved.chatSessions) || saved.chatSessions.length === 0) return false;

    state.chatSessions = saved.chatSessions
      .filter((session) => session?.id && Array.isArray(session.blocks))
      .slice(0, 24);
    if (state.chatSessions.length === 0) return false;

    state.activeChatId = state.chatSessions.some((session) => session.id === saved.activeChatId)
      ? saved.activeChatId
      : state.chatSessions[0].id;
    const active = state.chatSessions.find((session) => session.id === state.activeChatId);
    state.chatBlocks = cloneBlocks(active?.blocks?.length ? active.blocks : openingChatBlocks);
    state.chatSessions = sortChatSessions(state.chatSessions);
    return true;
  } catch (_error) {
    try {
      window.localStorage.removeItem(CHAT_HISTORY_STORAGE_KEY);
    } catch (_innerError) {
      // Ignore storage cleanup failures.
    }
    return false;
  }
}

export function hydrateUiPreferences() {
  try {
    const storage = typeof window === 'undefined' ? null : window.localStorage;
    if (!storage) return;
    const saved = JSON.parse(storage.getItem(UI_PREFERENCES_STORAGE_KEY) || 'null');
    if (!saved || typeof saved !== 'object') return;

    if (fontOptions.ui.some((option) => option.css === saved.uiFontFamily)) {
      state.uiFontFamily = saved.uiFontFamily;
    }
    if (fontOptions.mono.some((option) => option.css === saved.codeFontFamily)) {
      state.codeFontFamily = saved.codeFontFamily;
    }
  } catch (_error) {
    // UI preference persistence is optional.
  }
}

function persistUiPreferences() {
  try {
    const storage = typeof window === 'undefined' ? null : window.localStorage;
    if (!storage) return;
    storage.setItem(
      UI_PREFERENCES_STORAGE_KEY,
      JSON.stringify({
        uiFontFamily: state.uiFontFamily,
        codeFontFamily: state.codeFontFamily,
      }),
    );
  } catch (_error) {
    // UI preference persistence is optional.
  }
}

export function setUiPreference(key, value) {
  if (key === 'uiFontFamily' && fontOptions.ui.some((option) => option.css === value)) {
    state.uiFontFamily = value;
  }

  if (key === 'codeFontFamily' && fontOptions.mono.some((option) => option.css === value)) {
    state.codeFontFamily = value;
  }

  persistUiPreferences();
}

export function setProvider(provider) {
  state.llmProvider = provider.id;
  state.providerApi = provider.api;
  state.providerBaseUrl = provider.defaultBaseUrl;
  const allowedAuthMethods = provider.authMethods || ['api-key'];
  if (!allowedAuthMethods.includes(state.providerAuthMethod)) {
    state.providerAuthMethod = allowedAuthMethods[0];
  }
  state.providerModel = defaultModelForAuth(provider, state.providerAuthMethod);
  state.customApiKeyEnv = provider.apiKeyEnv;
  state.apiTest = {
    status: 'idle',
    message: 'Provider changed. Test it before finishing setup.',
  };
}

export function ensureProviderModelAllowed() {
  const provider = providerPresets.find((item) => item.id === state.llmProvider) || providerPresets[0];
  if (!modelAllowedForAuth(provider, state.providerModel, state.providerAuthMethod)) {
    state.providerModel = defaultModelForAuth(provider, state.providerAuthMethod);
  }
}

export function setChannel(channelId, enabled) {
  const channels = new Set(state.selectedChannels);
  channels.add('local');
  if (enabled) {
    channels.add(channelId);
  } else if (channelId !== 'local') {
    channels.delete(channelId);
  }
  state.selectedChannels = [...channels];
}

export function appendChatMessage(role, body) {
  state.chatBlocks.push({
    type: 'message',
    role,
    title: role === 'user' ? 'You' : state.agentName || 'Argentum',
    body,
  });
  syncActiveChatSession();
}
