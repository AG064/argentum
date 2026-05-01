import { APP_VERSION, providerPresets } from './constants.js';

const defaultProvider = providerPresets.find((provider) => provider.id === 'openai');

export const state = {
  activeSection: 'onboarding',
  onboardingStep: 1,
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
  providerApiKey: '',
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
  savedConfigPath: '',
  actionStatus: 'No GUI action has run in this session.',
  copiedCommand: '',
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
      body: 'I am ready to help set up your local workspace. I will stay inside the folder you choose unless you approve more access.',
    },
    {
      type: 'optionGroup',
      title: 'First preference',
      body: 'What should I help you configure first?',
      options: [
        { id: 'name-agent', label: 'Name the agent', detail: 'Pick the name and tone for this Argentum workspace.' },
        { id: 'security-policy', label: 'Security policy', detail: 'Choose when Argentum should ask before acting.' },
        { id: 'features', label: 'Feature set', detail: 'Decide which tools and channels should be enabled.' },
      ],
    },
  ],
  draftMessage: '',
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
  state.notifications = [{ id, type, title, message }, ...state.notifications].slice(0, 4);
  return id;
}

export function dismissNotification(id) {
  state.notifications = state.notifications.filter((notification) => notification.id !== id);
}

export function setProvider(provider) {
  state.llmProvider = provider.id;
  state.providerApi = provider.api;
  state.providerBaseUrl = provider.defaultBaseUrl;
  state.providerModel = provider.defaultModel;
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
    title: role === 'user' ? 'You' : 'Argentum',
    body,
  });
}
