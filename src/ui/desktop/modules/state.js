import { APP_VERSION, fontOptions, providerPresets } from './constants.js';
import { parseReasoningBlocks } from './reasoning-parser.js';
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
  const lastMessage = [...blocks].reverse().find((block) => block.type === 'message' && block.body);
  if (!lastMessage) return 'No messages yet';

  const text = String(lastMessage.body).replace(/\s+/g, ' ').trim();
  return text.length > 68 ? `${text.slice(0, 65)}...` : text;
}

function titleFromChat(blocks, fallback) {
  const firstUserMessage = blocks.find(
    (block) => block.type === 'message' && block.role === 'user',
  );
  if (!firstUserMessage?.body) return fallback;

  const text = String(firstUserMessage.body).replace(/\s+/g, ' ').trim();
  return text.length > 34 ? `${text.slice(0, 31)}...` : text;
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function redactPrivateText(text) {
  let output = String(text || '');
  const systemPrompt = String(state.systemPrompt || '').trim();
  if (systemPrompt && output.includes(systemPrompt)) {
    output = output.split(systemPrompt).join('[system prompt hidden]');
  }

  for (const [label, value, replacement] of [
    ['Agent name', state.agentName, '[agent name hidden]'],
    ['User name', state.userName, '[user name hidden]'],
  ]) {
    const current = String(value || '').trim();
    if (!current) continue;
    output = output.replace(
      new RegExp(`(${label}\\s*[:=]\\s*)${escapeRegExp(current)}`, 'gi'),
      `$1${replacement}`,
    );
  }

  return output;
}

function splitReasoningFromMessage(body) {
  const parsed = parseReasoningBlocks(body);
  if (!parsed.rawBody.trim()) {
    return { rawBody: parsed.rawBody, body: '', reasoning: '' };
  }
  const visible = parsed.body;

  return {
    rawBody: parsed.rawBody,
    body: redactPrivateText(
      visible ||
        'I completed the reasoning, but the provider did not return a separate visible answer.',
    ),
    reasoning: redactPrivateText(parsed.reasoning),
  };
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
  onboardingError: '',
  onboardingValidationErrors: [],
  onboardingDebug: null,
  onboardingProgressLoaded: false,
  setupComplete: false,
  setupStatus: 'setup_pending',
  setupAnimation: false,
  version: APP_VERSION,
  workspacePath: '',
  experienceLevel: '',
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
  llamaServerConfig: {
    modelSource: 'huggingface',
    modelPreset: 'qwen2.5-0.5b-instruct-q4',
    modelPath: '',
    hfRepo: 'Qwen/Qwen2.5-0.5B-Instruct-GGUF:Q4_K_M',
    hfFile: 'qwen2.5-0.5b-instruct-q4_k_m.gguf',
    host: '127.0.0.1',
    port: 8080,
    contextSize: 16384,
    gpuLayers: 999,
    threads: 10,
    temperature: 0.7,
    topP: 0.95,
    repeatPenalty: 1.1,
    batchSize: 1024,
    ubatchSize: 256,
    parallelSlots: 1,
    cpuMoe: 22,
    timeout: 0,
    cacheTypeK: 'f16',
    cacheTypeV: 'f16',
    flashAttention: true,
    noMmap: true,
    mlock: true,
    jinja: true,
  },
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
      message:
        'Choose a workspace folder first. Argentum will keep default access inside that folder.',
    },
  ],
  notificationHistory: [
    {
      id: 'welcome',
      type: 'info',
      title: 'Welcome',
      message:
        'Choose a workspace folder first. Argentum will keep default access inside that folder.',
    },
  ],
  appLogEntries: [],
  notificationsMuted: false,
  notificationsMenuOpen: false,
  quickSecurityMenuOpen: false,
  quickSettingsMenuOpen: false,
  helpOpen: false,
  workspaceMenuOpen: false,
  viewMode: 'chat',
  chatFilter: 'all',
  conversationsCollapsed: false,
  inspectorCollapsed: false,
  conversationMenuChatId: '',
  chatAutoFollow: true,
  chatHasNewTransmission: false,
  settingsSection: 'overview',
  uiFontFamily: fontOptions.ui[0].css,
  codeFontFamily: fontOptions.mono[0].css,
  uiLanguage: 'en',
  accentColor: '',
  highContrastMode: false,
  updateAvailable: false,
  updateVersion: '',
  updateDownloading: false,
  updateProgress: 0,
  updateError: '',
  savedConfigPath: '',
  actionStatus: 'No GUI action has run in this session.',
  runningAction: '',
  llamaServerProgress: null,
  localServerGuideOpen: false,
  copiedCommand: '',
  userName: '',
  userAvatarPath: '',
  agentName: 'Argentum',
  systemPrompt:
    'You are Argentum: a local-first developer agent. Be precise, useful, and honest about uncertainty. Work only within approved workspace permissions, prefer small verifiable steps, surface errors plainly, and propose durable CORE or skill-memory updates when they would help future work.',
  agentPurpose: '',
  thinkingLevel: 'balanced',
  showThinkingInChat: false,
  showThinkingInTelegram: false,
  chatStreaming: false,
  activeAssistantMessageId: '',
  chatAbortRequested: false,
  chatScrollIntent: '',
  chatAttachments: [],
  voiceInputStatus: 'idle',
  selectedContextAccess: ['workspace-summary', 'profile', 'tool-state'],
  onboardingOpenDisclosures: [],
  activeChatId: 'setup',
  pendingDeleteChatId: '',
  chatSessions: [
    {
      id: 'setup',
      title: 'Setup and security',
      subtitle: 'Workspace, provider, and permissions',
      blocks: cloneBlocks(openingChatBlocks),
      updatedAt: Date.now(),
      lastMessageAt: Date.now(),
      selectedAt: Date.now(),
      lastOpenedAt: Date.now(),
      pinned: false,
      unreadCount: 0,
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
      lastMessageAt: Date.now() - 1,
      selectedAt: 0,
      lastOpenedAt: 0,
      pinned: false,
      unreadCount: 0,
    },
  ],
  desktopState: {
    workspaceReady: false,
    configExists: false,
    dataExists: false,
    logsExists: false,
    gatewayPid: null,
    llamaServerInstalled: false,
    llamaServerPid: null,
    llamaServerEndpoint: 'http://127.0.0.1:8080/v1',
    llamaServerLogPreview: 'No entries yet.',
    gatewayLogPreview: 'No entries yet.',
    auditLogPreview: 'No entries yet.',
    appLogPreview: 'No entries yet.',
    systemStats: null,
  },
  chatBlocks: cloneBlocks(openingChatBlocks),
  draftMessage: '',
  terminalEntries: [
    {
      id: 'terminal-boot',
      status: 'info',
      command: 'argentum desktop',
      output:
        'Desktop shell loaded. Gateway, chat, diagnostics, and setup output will appear here when actions run.',
    },
  ],
  // Migration
  migrationSources: { openclaw: null, hermes: null },
  migrationDetected: false,
  migrationSkipped: false, // true if user skipped during onboarding
  migrationInProgress: false,
  migrationResults: null,
  migrationError: '',

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
  // Skills catalog
  skillsCatalog: { argentum: [], anthropic: [], codex: [] },
  installedSkills: [],
  skillsTab: 'argentum',
  skillsSearch: '',
  skillsCategory: 'all',
};

// Secure random ID generator (replaces Math.random() for security-sensitive IDs)
function generateSecureId(length = 16) {
  const array = new Uint8Array(length);
  require('crypto').getRandomValues(array);
  return Array.from(array, (b) => b.toString(16).padStart(2, '0'))
    .join('')
    .slice(0, length);
}

export function notify(type, title, message) {
  const id = `${Date.now()}-${generateSecureId()}`;
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

export function setViewMode(mode) {
  const nextMode = ['chat', 'split', 'full'].includes(mode) ? mode : 'chat';
  if (state.viewMode === nextMode) return;
  state.viewMode = nextMode;
  recordUiEvent('view.mode_changed', 'ok', `View mode changed to ${nextMode}.`, {
    viewMode: nextMode,
  });
  persistUiPreferences();
}

export function toggleChatPanel(panel, open) {
  const key =
    panel === 'conversations'
      ? 'conversationsCollapsed'
      : panel === 'inspector'
        ? 'inspectorCollapsed'
        : '';
  if (!key) return;
  const nextCollapsed = typeof open === 'boolean' ? !open : !state[key];
  state[key] = nextCollapsed;
  recordUiEvent(
    'chat.panel_visibility_changed',
    'ok',
    `${panel} panel ${nextCollapsed ? 'hidden' : 'shown'}.`,
    { panel, collapsed: nextCollapsed },
  );
  persistUiPreferences();
}

export function setChatFilter(filter) {
  const nextFilter = ['pinned', 'all'].includes(filter) ? filter : 'all';
  state.chatFilter = nextFilter;
  state.conversationMenuChatId = '';
  recordUiEvent('chat.filter_changed', 'ok', `Conversation filter changed to ${nextFilter}.`, {
    filter: nextFilter,
  });
}

export function toggleConversationMenu(chatId) {
  state.conversationMenuChatId = state.conversationMenuChatId === chatId ? '' : chatId;
}

export function toggleChatPinned(chatId) {
  const chat = state.chatSessions.find((session) => session.id === chatId);
  if (!chat) return;
  chat.pinned = !chat.pinned;
  chat.updatedAt = Date.now();
  state.conversationMenuChatId = '';
  recordUiEvent(
    'chat.pin_changed',
    'ok',
    `${chat.pinned ? 'Pinned' : 'Unpinned'} ${chat.title || chat.id}.`,
    {
      chatId,
      pinned: chat.pinned,
    },
  );
  persistChatHistory();
}

export function clearChatSession(chatId) {
  const chat = state.chatSessions.find((session) => session.id === chatId);
  if (!chat) return;
  const now = Date.now();
  chat.blocks = cloneBlocks(openingChatBlocks);
  chat.subtitle = 'No messages yet';
  chat.updatedAt = now;
  chat.lastMessageAt = now;
  chat.unreadCount = 0;
  state.conversationMenuChatId = '';
  if (state.activeChatId === chatId) {
    state.chatBlocks = cloneBlocks(openingChatBlocks);
  }
  recordUiEvent('chat.cleared', 'ok', `Cleared ${chat.title || chat.id}.`, { chatId });
  state.chatSessions = sortChatSessions(state.chatSessions);
  persistChatHistory();
}

export function renameChatSession(chatId, title) {
  const chat = state.chatSessions.find((session) => session.id === chatId);
  const cleanTitle = String(title || '').trim();
  if (!chat || !cleanTitle) return;
  chat.title = cleanTitle.slice(0, 80);
  chat.updatedAt = Date.now();
  state.conversationMenuChatId = '';
  recordUiEvent('chat.renamed', 'ok', `Renamed chat to ${chat.title}.`, { chatId });
  persistChatHistory();
}

export function toggleHelp(open) {
  state.helpOpen = typeof open === 'boolean' ? open : !state.helpOpen;
  if (state.helpOpen) {
    recordUiEvent('help.opened', 'ok', `Opened help for ${state.activeSection}.`, {
      section: state.activeSection,
    });
  }
}

export function toggleWorkspaceMenu(open) {
  state.workspaceMenuOpen = typeof open === 'boolean' ? open : !state.workspaceMenuOpen;
  if (state.workspaceMenuOpen) {
    recordUiEvent('workspace.menu_opened', 'ok', 'Opened workspace menu.', {
      workspacePath: state.workspacePath,
    });
  }
}

export function setSettingsSection(sectionId) {
  const allowed = [
    'overview',
    'workspace',
    'provider',
    'model',
    'local-server',
    'context',
    'chat',
    'telegram',
    'security',
    'advanced',
    'appearance',
    'skills',
  ];
  state.settingsSection = allowed.includes(sectionId) ? sectionId : 'overview';
  recordUiEvent('settings.section_opened', 'ok', `Opened ${state.settingsSection} settings.`, {
    section: state.settingsSection,
  });
}

export function recordUiEvent(event, status, message, details = {}) {
  const entry = {
    id: generateSecureId(),
    event,
    status,
    message,
    details,
    timestamp: new Date().toISOString(),
  };
  state.appLogEntries = [entry, ...state.appLogEntries].slice(0, 250);

  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event('argentum:state-change'));
  }

  return entry;
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

export function toggleNotificationsMenu(open) {
  state.notificationsMenuOpen = typeof open === 'boolean' ? open : !state.notificationsMenuOpen;
  if (state.notificationsMenuOpen) {
    state.quickSecurityMenuOpen = false;
    state.quickSettingsMenuOpen = false;
  }
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event('argentum:state-change'));
  }
}

export function toggleQuickSecurityMenu(open) {
  state.quickSecurityMenuOpen = typeof open === 'boolean' ? open : !state.quickSecurityMenuOpen;
  if (state.quickSecurityMenuOpen) {
    state.notificationsMenuOpen = false;
    state.quickSettingsMenuOpen = false;
    recordUiEvent('security.quick_menu_opened', 'ok', 'Opened current-view security controls.', {
      section: state.activeSection,
      securityProfile: state.securityProfile,
    });
  }
}

export function toggleQuickSettingsMenu(open) {
  state.quickSettingsMenuOpen = typeof open === 'boolean' ? open : !state.quickSettingsMenuOpen;
  if (state.quickSettingsMenuOpen) {
    state.notificationsMenuOpen = false;
    state.quickSecurityMenuOpen = false;
    recordUiEvent('settings.quick_menu_opened', 'ok', 'Opened current-view quick settings.', {
      section: state.activeSection,
    });
  }
}

export function addTerminalEntry(command, output, status = 'info') {
  state.terminalEntries = [
    {
      id: generateSecureId(),
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
      if (Boolean(a.pinned) !== Boolean(b.pinned)) return a.pinned ? -1 : 1;
      return (b.lastMessageAt || b.updatedAt || 0) - (a.lastMessageAt || a.updatedAt || 0);
    })
    .slice(0, 24);
}

function normalizeChatBlocks(blocks, channel = '') {
  if (!Array.isArray(blocks)) return [];
  return blocks
    .filter((block) => block && typeof block === 'object' && block.body)
    .map((block) => {
      const role = block.role === 'user' ? 'user' : 'argentum';
      const parsed =
        role === 'user'
          ? {
              body: block.body,
              reasoning: block.reasoning || '',
              rawBody: block.rawBody || block.body,
            }
          : splitReasoningFromMessage(block.rawBody || block.body);
      return {
        type: block.type || 'message',
        role,
        title:
          block.title ||
          (role === 'user'
            ? 'You'
            : channel === 'telegram'
              ? 'Telegram'
              : state.agentName || 'Argentum'),
        body: role === 'user' ? redactPrivateText(block.body) : parsed.body,
        reasoning: role === 'user' ? redactPrivateText(block.reasoning || '') : parsed.reasoning,
        rawBody: parsed.rawBody,
        status: block.status || 'sent',
        createdAt: block.createdAt || Date.now(),
        attachments: Array.isArray(block.attachments) ? block.attachments : [],
      };
    });
}

export function mergeChannelChatSessions(channelSessions = []) {
  if (!Array.isArray(channelSessions) || channelSessions.length === 0) return;

  const byId = new Map(state.chatSessions.map((session) => [session.id, session]));
  for (const raw of channelSessions) {
    if (!raw?.id) continue;
    const channel = raw.channel === 'telegram' ? 'telegram' : String(raw.channel || 'external');
    const blocks = normalizeChatBlocks(raw.blocks, channel);
    const existing = byId.get(raw.id);
    const lastMessageAt = Number(
      raw.lastMessageAt ||
        raw.updatedAt ||
        existing?.lastMessageAt ||
        existing?.updatedAt ||
        Date.now(),
    );
    const lastOpenedAt = Number(existing?.lastOpenedAt || 0);
    const unreadCount =
      raw.id === state.activeChatId
        ? 0
        : lastMessageAt > lastOpenedAt
          ? Math.max(Number(existing?.unreadCount || 0), 1)
          : Number(existing?.unreadCount || 0);
    const session = {
      ...existing,
      id: raw.id,
      channel,
      title:
        raw.title ||
        existing?.title ||
        (channel === 'telegram' ? 'Telegram chat' : 'External chat'),
      subtitle:
        raw.subtitle ||
        summarizeChat(blocks) ||
        existing?.subtitle ||
        'Imported channel conversation',
      blocks: blocks.length > 0 ? blocks : existing?.blocks || cloneBlocks(openingChatBlocks),
      updatedAt: Number(raw.updatedAt || existing?.updatedAt || Date.now()),
      lastMessageAt,
      selectedAt: Number(existing?.selectedAt || 0),
      lastOpenedAt: raw.id === state.activeChatId ? Date.now() : lastOpenedAt,
      pinned: Boolean(raw.pinned || existing?.pinned),
      unreadCount,
    };
    byId.set(session.id, session);

    if (session.id === state.activeChatId) {
      state.chatBlocks = cloneBlocks(session.blocks);
    }
  }

  state.chatSessions = sortChatSessions([...byId.values()]);
  persistChatHistory();
}

export function touchActiveChatSession() {
  const index = state.chatSessions.findIndex((chat) => chat.id === state.activeChatId);
  if (index !== -1) {
    state.chatSessions[index] = {
      ...state.chatSessions[index],
      updatedAt: Date.now(),
      lastMessageAt: Date.now(),
    };
  }
  state.chatSessions = sortChatSessions(state.chatSessions);
  persistChatHistory();
}

export function syncActiveChatSession(options = {}) {
  const index = state.chatSessions.findIndex((chat) => chat.id === state.activeChatId);
  if (index === -1) return;

  const current = state.chatSessions[index];
  const now = Date.now();
  const fallbackTitle = current.title || 'New chat';
  state.chatSessions[index] = {
    ...current,
    title: titleFromChat(
      state.chatBlocks,
      fallbackTitle === 'New chat' ? 'New chat' : fallbackTitle,
    ),
    subtitle: summarizeChat(state.chatBlocks),
    blocks: cloneBlocks(state.chatBlocks),
    updatedAt: now,
    lastMessageAt: options.messageActivity
      ? now
      : current.lastMessageAt || current.updatedAt || now,
  };
  state.chatSessions = sortChatSessions(state.chatSessions);
  persistChatHistory();
}

export function setActiveChatSession(chatId) {
  syncActiveChatSession();
  const session = state.chatSessions.find((chat) => chat.id === chatId) || state.chatSessions[0];
  if (!session) return;

  const now = Date.now();
  state.activeChatId = session.id;
  session.selectedAt = now;
  session.lastOpenedAt = now;
  session.unreadCount = 0;
  state.chatAutoFollow = true;
  state.chatHasNewTransmission = false;
  state.chatBlocks = cloneBlocks(session.blocks?.length ? session.blocks : openingChatBlocks);
  state.draftMessage = '';
  state.chatAttachments = [];
  recordUiEvent('chat.selected', 'ok', `Selected chat ${session.title || session.id}.`, {
    chatId: session.id,
  });
  persistChatHistory();
}

export function createChatSession() {
  syncActiveChatSession();
  const id = `chat-${Date.now()}-${generateSecureId()}`;
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
    lastMessageAt: Date.now(),
    selectedAt: Date.now(),
    lastOpenedAt: Date.now(),
    pinned: false,
    unreadCount: 0,
  };

  state.chatSessions = [session, ...state.chatSessions].slice(0, 24);
  state.activeChatId = id;
  state.chatBlocks = cloneBlocks(session.blocks);
  state.draftMessage = '';
  state.chatAttachments = [];
  state.chatAutoFollow = true;
  state.chatHasNewTransmission = false;
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
      lastMessageAt: Date.now(),
      selectedAt: Date.now(),
      lastOpenedAt: Date.now(),
      pinned: false,
      unreadCount: 0,
    };
    state.chatSessions = [replacement];
  } else {
    state.chatSessions = remaining;
  }

  state.pendingDeleteChatId = '';
  if (!state.chatSessions.some((chat) => chat.id === state.activeChatId)) {
    state.activeChatId = state.chatSessions[0].id;
    state.chatBlocks = cloneBlocks(
      state.chatSessions[0].blocks?.length ? state.chatSessions[0].blocks : openingChatBlocks,
    );
  }

  state.chatSessions = sortChatSessions(state.chatSessions);
  persistChatHistory();
}

export function setProviderCatalogTab(tabId) {
  state.providerCatalogTab = tabId === 'testing' || tabId === 'beta' ? 'testing' : 'stable';
}

export function hydrateChatHistory() {
  try {
    const storage = typeof window === 'undefined' ? null : window.localStorage;
    if (!storage) return false;
    const saved = JSON.parse(storage.getItem(CHAT_HISTORY_STORAGE_KEY) || 'null');
    if (!saved || !Array.isArray(saved.chatSessions) || saved.chatSessions.length === 0)
      return false;

    state.chatSessions = saved.chatSessions
      .filter((session) => session?.id && Array.isArray(session.blocks))
      .slice(0, 24)
      .map((session) => {
        const updatedAt = Number(session.updatedAt || Date.now());
        const lastMessageAt = Number(session.lastMessageAt || updatedAt);
        return {
          ...session,
          blocks: normalizeChatBlocks(session.blocks, session.channel || ''),
          updatedAt,
          lastMessageAt,
          selectedAt: Number(session.selectedAt || 0),
          lastOpenedAt: Number(session.lastOpenedAt || 0),
          pinned: Boolean(session.pinned),
          unreadCount: Number(session.unreadCount || 0),
        };
      });
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
    if (typeof saved.accentColor === 'string' && saved.accentColor.trim()) {
      state.accentColor = saved.accentColor;
    }
    if (typeof saved.workspacePath === 'string' && saved.workspacePath.trim()) {
      state.workspacePath = saved.workspacePath;
    }
    if (['chat', 'split', 'full'].includes(saved.viewMode)) {
      state.viewMode = saved.viewMode;
    }
    if (['pinned', 'all'].includes(saved.chatFilter)) {
      state.chatFilter = saved.chatFilter;
    }
    if (typeof saved.conversationsCollapsed === 'boolean') {
      state.conversationsCollapsed = saved.conversationsCollapsed;
    }
    if (typeof saved.inspectorCollapsed === 'boolean') {
      state.inspectorCollapsed = saved.inspectorCollapsed;
    }
    if (typeof saved.userAvatarPath === 'string') {
      state.userAvatarPath = saved.userAvatarPath;
    }
    if (typeof saved.uiLanguage === 'string' && saved.uiLanguage.trim()) {
      state.uiLanguage = saved.uiLanguage;
    }
    if (typeof saved.setupComplete === 'boolean') {
      state.setupComplete = saved.setupComplete;
      state.onboardingOpen = !saved.setupComplete;
    }
    if (typeof saved.savedConfigPath === 'string') {
      state.savedConfigPath = saved.savedConfigPath;
    }
    if (saved.llamaServerConfig && typeof saved.llamaServerConfig === 'object') {
      state.llamaServerConfig = {
        ...state.llamaServerConfig,
        ...saved.llamaServerConfig,
      };
      if (
        !saved.llamaServerConfig.modelSource &&
        state.llamaServerConfig.modelPath === 'models/argentum-default.gguf'
      ) {
        state.llamaServerConfig = {
          ...state.llamaServerConfig,
          modelSource: 'huggingface',
          modelPreset: 'qwen2.5-0.5b-instruct-q4',
          modelPath: '',
          hfRepo: 'Qwen/Qwen2.5-0.5B-Instruct-GGUF:Q4_K_M',
          hfFile: 'qwen2.5-0.5b-instruct-q4_k_m.gguf',
        };
      }
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
        accentColor: state.accentColor,
        workspacePath: state.workspacePath,
        viewMode: state.viewMode,
        chatFilter: state.chatFilter,
        conversationsCollapsed: state.conversationsCollapsed,
        inspectorCollapsed: state.inspectorCollapsed,
        userAvatarPath: state.userAvatarPath,
        uiLanguage: state.uiLanguage,
        setupComplete: state.setupComplete,
        savedConfigPath: state.savedConfigPath,
        llamaServerConfig: state.llamaServerConfig,
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

  if (key === 'accentColor') {
    state.accentColor = typeof value === 'string' ? value.trim() : '';
  }

  if (key === 'workspacePath' && typeof value === 'string' && value.trim()) {
    state.workspacePath = value.trim();
  }

  if (key === 'userAvatarPath' && typeof value === 'string') {
    state.userAvatarPath = value.trim();
  }

  if (key === 'uiLanguage' && typeof value === 'string' && value.trim()) {
    state.uiLanguage = value.trim();
  }

  persistUiPreferences();
}

export function setLlamaServerConfig(key, value) {
  state.llamaServerConfig = {
    ...state.llamaServerConfig,
    [key]: value,
  };
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
  const provider =
    providerPresets.find((item) => item.id === state.llmProvider) || providerPresets[0];
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

export function setOnboardingDisclosure(disclosureId, open) {
  const current = new Set(state.onboardingOpenDisclosures || []);
  if (open) current.add(disclosureId);
  else current.delete(disclosureId);
  state.onboardingOpenDisclosures = [...current];
}

export function compactActiveChatSession(options = {}) {
  const messageBlocks = state.chatBlocks.filter((block) => block.type === 'message');
  const nonMessageBlocks = state.chatBlocks.filter((block) => block.type !== 'message');
  const keepCount = Math.max(4, Number(options.keepCount || 12));
  if (messageBlocks.length <= keepCount + 2) return false;

  const older = messageBlocks.slice(0, Math.max(0, messageBlocks.length - keepCount));
  const recent = messageBlocks.slice(-keepCount);
  const summary = older
    .map((block) => {
      const speaker = block.role === 'user' ? 'User' : state.agentName || 'Argentum';
      return `${speaker}: ${String(block.body || '')
        .replace(/\s+/g, ' ')
        .trim()}`;
    })
    .join('\n')
    .slice(-1800);

  const compactedBlock = {
    id: `compact-${Date.now()}-${generateSecureId()}`,
    type: 'summary',
    title: options.automatic ? 'Auto-compacted context' : 'Compacted context',
    body: `Earlier conversation was compacted locally to keep this session responsive.\n\n${summary}`,
    status: 'sent',
    createdAt: Date.now(),
  };

  state.chatBlocks = [...nonMessageBlocks, compactedBlock, ...recent];
  syncActiveChatSession({ messageActivity: false });
  recordUiEvent(
    options.automatic ? 'chat.context_auto_compacted' : 'chat.context_compacted',
    'ok',
    options.automatic
      ? 'Conversation context was auto-compacted.'
      : 'Conversation context was compacted by the user.',
    {
      chatId: state.activeChatId,
      compactedMessages: older.length,
    },
  );
  return true;
}

export function appendChatMessage(role, body, options = {}) {
  const sourceBody = Object.prototype.hasOwnProperty.call(options, 'rawBody')
    ? options.rawBody
    : body;
  const parsed =
    role === 'user'
      ? { rawBody: String(sourceBody || ''), body: redactPrivateText(sourceBody), reasoning: '' }
      : splitReasoningFromMessage(sourceBody);
  const block = {
    id: options.id || `${Date.now()}-${generateSecureId()}`,
    type: 'message',
    role,
    title: role === 'user' ? 'You' : state.agentName || 'Argentum',
    rawBody: parsed.rawBody,
    body: parsed.body,
    reasoning: Object.prototype.hasOwnProperty.call(options, 'reasoning')
      ? options.reasoning
      : parsed.reasoning,
    status: options.status || 'sent',
    createdAt: options.createdAt || Date.now(),
    attachments: Array.isArray(options.attachments) ? options.attachments : [],
    error: options.error || '',
  };
  state.chatBlocks.push(block);
  syncActiveChatSession({ messageActivity: true });
  return block;
}

export function updateChatMessage(messageId, patch = {}) {
  const index = state.chatBlocks.findIndex((block) => block.id === messageId);
  if (index === -1) return null;
  const current = state.chatBlocks[index];
  const rawBody = Object.prototype.hasOwnProperty.call(patch, 'rawBody')
    ? patch.rawBody
    : Object.prototype.hasOwnProperty.call(patch, 'body')
      ? patch.body
      : current.rawBody || current.body;
  const parsed =
    current.role === 'user'
      ? {
          rawBody: String(rawBody || ''),
          body: redactPrivateText(rawBody),
          reasoning: current.reasoning || '',
        }
      : splitReasoningFromMessage(rawBody);
  state.chatBlocks[index] = {
    ...current,
    ...patch,
    rawBody: parsed.rawBody,
    body: parsed.body,
    reasoning: Object.prototype.hasOwnProperty.call(patch, 'reasoning')
      ? patch.reasoning
      : parsed.reasoning,
  };
  syncActiveChatSession({
    messageActivity:
      patch.status === 'sent' || patch.status === 'stopped' || patch.messageActivity === true,
  });
  return state.chatBlocks[index];
}

// Skills catalog state setters
export function setSkillsCatalog(anthropic, codex) {
  state.skillsCatalog = { anthropic, codex };
}

export function setInstalledSkills(skills) {
  state.installedSkills = skills;
}

export function setSkillsTab(tab) {
  state.skillsTab = tab;
}

export function setSkillsSearch(query) {
  state.skillsSearch = query;
}

export function setSkillsCategory(category) {
  state.skillsCategory = category;
}
