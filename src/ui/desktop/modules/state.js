import { APP_VERSION, providerPresets } from './constants.js';

const defaultProvider = providerPresets.find((provider) => provider.id === 'openai');

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
  savedConfigPath: '',
  actionStatus: 'No GUI action has run in this session.',
  runningAction: '',
  copiedCommand: '',
  userName: '',
  agentName: 'Argentum',
  agentPurpose: '',
  thinkingLevel: 'balanced',
  chatAttachments: [],
  voiceInputStatus: 'idle',
  recentChats: [
    { id: 'setup', title: 'Setup and security', subtitle: 'Workspace, provider, and permissions' },
    { id: 'general', title: 'General chat', subtitle: 'Ask Argentum directly' },
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
  chatBlocks: [
    {
      type: 'message',
      role: 'argentum',
      title: 'Argentum',
      body: 'I am ready. Finish onboarding when it appears, then we can tune your profile, provider, and workspace permissions from here.',
    },
  ],
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

export function setProvider(provider) {
  state.llmProvider = provider.id;
  state.providerApi = provider.api;
  state.providerBaseUrl = provider.defaultBaseUrl;
  state.providerModel = provider.defaultModel;
  const allowedAuthMethods = provider.authMethods || ['api-key'];
  if (!allowedAuthMethods.includes(state.providerAuthMethod)) {
    state.providerAuthMethod = allowedAuthMethods[0];
  }
  state.customApiKeyEnv = provider.apiKeyEnv;
  state.apiTest = {
    status: 'idle',
    message: 'Provider changed. Test it before finishing setup.',
  };
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
}
