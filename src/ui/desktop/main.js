import { commandCatalog, onboardingSteps, providerPresets, sections } from './modules/constants.js';
import { hydrateStaticIcons } from './modules/icons.js';
import { modules } from './modules/sections.js';
import { renderModule, renderNavigation } from './modules/shell.js';
import {
  addTerminalEntry,
  appendChatMessage,
  clearNotifications,
  createChatSession,
  dismissNotification,
  ensureProviderModelAllowed,
  hydrateChatHistory,
  notify,
  scheduleVisibleNotifications,
  setChannel,
  setActiveChatSession,
  setProvider,
  state,
  syncActiveChatSession,
  toggleNotificationsMenu,
  toggleNotificationsMuted,
} from './modules/state.js';
import {
  chooseWorkspaceFolder,
  completeCodexOAuth,
  openExternalUrl,
  refreshDesktopState,
  saveSetup,
  sendChatMessage,
  startCodexOAuth,
  testProvider,
  hydrateDesktopDefaults,
} from './modules/setup.js';
import { isProbablyAbsolutePath, normalizeError, openFile } from './modules/utils.js';

const nav = document.querySelector('#section-nav');
const title = document.querySelector('#section-title');
const eyebrow = document.querySelector('#eyebrow');
const viewRoot = document.querySelector('#view-root');
const workspacePath = document.querySelector('#workspace-path');

const chatActions = {
  'test-provider': { label: 'Test Provider' },
  'gateway-start': { actionId: 'gateway-start', label: 'Start Gateway' },
  'gateway-status': { actionId: 'gateway-status', label: 'Check Gateway' },
  settings: { section: 'settings', label: 'Open Settings' },
};

function activeSection() {
  return sections.find((section) => section.id === state.activeSection && section.id !== 'onboarding') || sections[1];
}

function render() {
  const section = activeSection();
  const module = modules[section.id] || modules.chat;

  workspacePath.textContent = state.workspacePath;
  title.textContent = section.title;
  eyebrow.textContent = section.eyebrow;
  nav.innerHTML = renderNavigation();
  viewRoot.innerHTML = `${renderModule(module)}${state.onboardingOpen ? renderModule(modules.onboarding) : ''}`;
  hydrateStaticIcons(document);
}

function setActiveSection(sectionId) {
  if (!modules[sectionId]) return;
  state.activeSection = sectionId;
  render();
}

function advanceOnboarding() {
  const module = modules.onboarding;
  const validationError = module.validate(state);
  if (validationError) {
    notify('error', 'Setup needs a fix', validationError);
    render();
    return;
  }

  state.onboardingStep = Math.min(onboardingSteps.length, state.onboardingStep + 1);
  render();
}

function restartOnboarding() {
  state.onboardingOpen = true;
  state.onboardingStep = 1;
  state.setupStatus = state.setupComplete ? 'setup_reviewing' : 'setup_pending';
  notify('info', 'Onboarding opened', 'The setup dialog is back on top. Finish or cancel to return to the app.');
  render();
}

function cancelOnboarding() {
  if (!state.setupComplete) return;
  state.onboardingOpen = false;
  notify('info', 'Onboarding closed', 'Your saved setup is still active.');
  render();
}

function resetIntroChat() {
  state.chatBlocks = [
    {
      type: 'message',
      role: 'argentum',
      title: state.agentName || 'Argentum',
      body: state.userName
        ? `Welcome back, ${state.userName}. Ask what is next, prepare a gateway, or adjust the profile fields in Settings.`
        : 'Setup is saved. Add your name and agent name in Settings, or ask what is next.',
    },
  ];
  syncActiveChatSession();
}

async function finishOnboarding() {
  if (!isProbablyAbsolutePath(state.workspacePath)) {
    notify(
      'error',
      'Choose a full folder path',
      'Workspace path must be absolute. Use Browse and choose a folder like C:\\Users\\You\\Argentum Workspace.',
    );
    state.onboardingStep = 2;
    render();
    return;
  }

  state.setupAnimation = true;
  render();

  try {
    const result = await saveSetup();
    state.setupComplete = true;
    state.setupStatus = 'setup_saved';
    state.savedConfigPath = result.configPath || result.config_path || '';
    state.providerApiKey = '';
    state.webchatToken = '';
    state.telegramToken = '';
    state.activeSection = 'chat';
    state.onboardingOpen = false;
    state.onboardingStep = onboardingSteps.length;
    resetIntroChat();
    addTerminalEntry('argentum setup save', `Configuration saved${state.savedConfigPath ? ` to ${state.savedConfigPath}` : ' in the selected workspace'}.`, 'success');
    notify('success', 'Setup complete', 'Onboarding is hidden. Chat is ready for the introductory phase.');
    await refreshDesktopState();
  } catch (error) {
    state.setupStatus = 'setup-error';
    notify('error', 'Setup could not be saved', normalizeError(error));
  } finally {
    state.setupAnimation = false;
    render();
  }
}

async function runAction(actionId) {
  const action = commandCatalog.find((item) => item.id === actionId);
  const command = action?.command || actionId;
  state.runningAction = actionId;
  state.actionStatus = `Running ${action?.title || actionId}...`;
  addTerminalEntry(command, 'Running through the desktop bridge. Nothing leaves the workspace without the matching permission gate.', 'info');
  render();

  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    state.actionStatus = `Preview mode: ${command} needs the installed desktop app.`;
    addTerminalEntry(command, 'Preview mode: the installed Tauri app is required to execute gateway actions.', 'warning');
    notify('warning', 'Desktop bridge unavailable', state.actionStatus);
    state.runningAction = '';
    render();
    return;
  }

  try {
    const result = await invoke('run_desktop_action', {
      request: { actionId, workspacePath: state.workspacePath },
    });
    const status = result?.status || 'ok';
    const output = result?.output || result?.message || 'Action completed without output.';
    const actualCommand = result?.command || command;
    state.actionStatus = result?.message || `${action?.title || actionId} completed.`;
    addTerminalEntry(actualCommand, output, status === 'error' ? 'error' : 'success');
    notify(status === 'stopped' ? 'info' : 'success', action?.title || 'Desktop action', state.actionStatus);
    await refreshDesktopState();
  } catch (error) {
    const message = normalizeError(error);
    addTerminalEntry(command, message, 'error');
    notify('error', 'Action failed', message);
  } finally {
    state.runningAction = '';
  }
  render();
}

async function runChatAction(actionId) {
  const action = chatActions[actionId];
  if (!action) return;

  if (action.section) {
    setActiveSection(action.section);
    return;
  }

  if (actionId === 'test-provider') {
    const result = await testProvider();
    appendChatMessage('argentum', result.message);
    render();
    return;
  }

  if (action.actionId) {
    await runAction(action.actionId);
  }
}

async function copyCommand(command) {
  state.copiedCommand = command;
  try {
    await navigator.clipboard?.writeText(command);
    notify('success', 'Command copied', command);
  } catch (_error) {
    notify('info', 'Command ready', command);
  }
  render();
}

function updateProviderFieldsFromPreset(providerId) {
  const provider = providerPresets.find((item) => item.id === providerId);
  if (!provider) return;
  setProvider(provider);
  state.providerSelectionConfirmed = true;
  state.providerSetupStage = 'auth';
}

async function chooseChatAttachment() {
  const selected = await openFile(state.workspacePath);
  if (!selected) {
    notify('info', 'No file attached', 'No file was selected.');
    return;
  }

  const path = Array.isArray(selected) ? selected[0] : selected;
  state.chatAttachments = [
    ...state.chatAttachments,
    {
      id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
      path,
      name: String(path).split(/[\\/]/).pop() || String(path),
    },
  ].slice(-6);
  notify('success', 'File attached', 'The selected file will be referenced in the next chat message.');
}

function startVoiceInput() {
  const Recognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!Recognition) {
    state.voiceInputStatus = 'unsupported';
    notify('error', 'Microphone unavailable', 'Voice dictation is not available in this desktop webview yet.');
    return;
  }

  const recognition = new Recognition();
  recognition.lang = 'en-US';
  recognition.interimResults = false;
  recognition.maxAlternatives = 1;
  state.voiceInputStatus = 'listening';
  notify('info', 'Listening', 'Speak now. Argentum will add the transcript to the chat box.');

  recognition.onresult = (event) => {
    const transcript = event.results?.[0]?.[0]?.transcript || '';
    state.draftMessage = `${state.draftMessage}${state.draftMessage ? ' ' : ''}${transcript}`.trim();
    state.voiceInputStatus = 'idle';
    render();
  };
  recognition.onerror = () => {
    state.voiceInputStatus = 'error';
    notify('error', 'Voice input failed', 'Microphone input could not be captured.');
    render();
  };
  recognition.onend = () => {
    if (state.voiceInputStatus === 'listening') state.voiceInputStatus = 'idle';
    render();
  };
  recognition.start();
}

function saveProfileFromInputs() {
  const userInput = document.querySelector('#profile-user-name');
  const agentInput = document.querySelector('#profile-agent-name');
  const purposeInput = document.querySelector('#profile-purpose');

  if (userInput instanceof HTMLInputElement) state.userName = userInput.value.trim();
  if (agentInput instanceof HTMLInputElement) state.agentName = agentInput.value.trim() || 'Argentum';
  if (purposeInput instanceof HTMLTextAreaElement) state.systemPrompt = purposeInput.value.trim();

  appendChatMessage(
    'argentum',
    `Profile saved. I will use **${state.agentName || 'Argentum'}** as the agent name${state.userName ? ` and call you **${state.userName}**` : ''}.`,
  );
  notify('success', 'Profile saved', 'The chat profile was updated locally.');
}

function buildLocalReply(draft) {
  const text = draft.trim();
  const lower = text.toLowerCase();
  const nameMatch = text.match(/\b(?:my name is|call me|i am|i'm)\s+([^.,!?]+)/i);

  if (nameMatch?.[1]) {
    state.userName = nameMatch[1].trim();
    return `Nice to meet you, ${state.userName}. I saved that locally. You can also set the agent name and purpose in the profile panel.`;
  }

  if (lower.includes('what') && lower.includes('next')) {
    return 'Next: save your profile fields, test the provider, then prepare the gateway if you want local web or API access. Security stays restricted to the workspace unless you approve more.';
  }

  if (lower.includes('provider') || lower.includes('api') || lower.includes('model')) {
    return `Provider status: ${state.apiTest.message} Use Settings or onboarding restart to edit endpoint, model, and key, then run Test API.`;
  }

  if (lower.includes('security') || lower.includes('permission') || lower.includes('access')) {
    return `Security status: ${state.securityProfile}. Default access is only the files and folders inside ${state.workspacePath}. External folders, shell, network, and repair actions require approval.`;
  }

  if (lower.includes('gateway') || lower.includes('terminal')) {
    return 'Use Start Gateway or Check Gateway and watch the terminal panel. In the installed Tauri app, those buttons run fixed gateway commands through the desktop bridge.';
  }

  if (lower === 'hi' || lower === 'hey' || lower.startsWith('hello')) {
    return `Hey${state.userName ? `, ${state.userName}` : ''}. I can help with setup, security, provider tests, or gateway prep while live AI is offline.`;
  }

  if (state.apiTest.status !== 'ok') {
    return 'I am in local guided mode because the provider is not live-ready yet. I can still help with setup, profile, security, channels, diagnostics, and terminal actions.';
  }

  return 'Provider settings look ready. If this is the installed desktop app, the next message will be routed through the configured provider; otherwise this preview remains local.';
}

async function sendChatDraft() {
  const draft = state.draftMessage.trim();
  if (!draft && state.chatAttachments.length === 0) return;

  const attachmentText =
    state.chatAttachments.length > 0
      ? `\n\nAttached files:\n${state.chatAttachments.map((file) => `- ${file.name}: ${file.path}`).join('\n')}`
      : '';
  const outgoingMessage = `${draft}${attachmentText}`.trim();

  appendChatMessage('user', outgoingMessage);
  state.draftMessage = '';
  state.chatAttachments = [];
  render();

  if (!state.setupComplete || !window.__TAURI__?.core?.invoke) {
    appendChatMessage('argentum', buildLocalReply(draft));
    render();
    return;
  }

  state.actionStatus = 'Sending chat message...';
  state.chatStreaming = true;
  render();

  try {
    const result = await sendChatMessage(outgoingMessage);
    if (result?.offline) {
      state.apiTest = {
        status: 'warning',
        message: result.message,
      };
    } else {
      state.apiTest = {
        status: 'ok',
        message: `${result.provider || 'Provider'} answered with ${result.model || 'the configured model'}.`,
      };
    }

    appendChatMessage('argentum', result?.message || buildLocalReply(draft));
  } catch (error) {
    const message = normalizeError(error);
    appendChatMessage('argentum', `Live chat failed: ${message}`);
    notify('error', 'Chat failed', message);
  } finally {
    state.actionStatus = 'Chat is ready.';
    state.chatStreaming = false;
    render();
  }
}

function handleInput(event) {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;

  if (target.id === 'workspace-input' || target.id === 'settings-workspace') {
    state.workspacePath = target.value;
    workspacePath.textContent = state.workspacePath;
    return;
  }

  if (target.id === 'provider-base-url' || target.id === 'settings-provider-base-url') {
    state.providerBaseUrl = target.value;
  }
  if (target.id === 'provider-api-key') state.providerApiKey = target.value;
  if (target.id === 'custom-provider-name') state.customProviderName = target.value;
  if (target.id === 'custom-api-key-env') state.customApiKeyEnv = target.value;
  if (target.id === 'webchat-token') state.webchatToken = target.value;
  if (target.id === 'telegram-token') state.telegramToken = target.value;
  if (target.id === 'telegram-allowlist') state.telegramAllowlist = target.value;
  if (target.id === 'whatsapp-phone-id') state.whatsappPhoneId = target.value;
  if (target.id === 'chat-draft') state.draftMessage = target.value;
  if (target.id === 'profile-user-name') state.userName = target.value;
  if (target.id === 'onboarding-user-name') state.userName = target.value;
  if (target.id === 'profile-agent-name') state.agentName = target.value || 'Argentum';
  if (target.id === 'onboarding-agent-name') state.agentName = target.value || 'Argentum';
  if (target.id === 'profile-purpose') state.systemPrompt = target.value;
  if (target.id === 'onboarding-system-prompt') state.systemPrompt = target.value;
}

function handleChange(event) {
  const target = event.target;
  if (!(target instanceof HTMLInputElement) && !(target instanceof HTMLSelectElement)) return;

  if (target.dataset.channelId) {
    setChannel(target.dataset.channelId, target.checked);
    render();
    return;
  }

  if (target.dataset.contextAccess) {
    const access = new Set(state.selectedContextAccess);
    if (target.checked) access.add(target.dataset.contextAccess);
    else access.delete(target.dataset.contextAccess);
    state.selectedContextAccess = [...access];
    render();
    return;
  }

  if (target.id === 'provider-api') {
    state.providerApi = target.value;
    render();
    return;
  }

  if (target.id === 'provider-model' || target.id === 'settings-provider-model') {
    state.providerModel = target.value;
    state.apiTest = {
      status: 'idle',
      message: 'Model changed. Test the provider before using live chat.',
    };
    render();
    return;
  }

  if (target.id === 'provider-auth-method' || target.id === 'settings-provider-auth-method') {
    state.providerAuthMethod = target.value;
    ensureProviderModelAllowed();
    if (target.id === 'provider-auth-method') state.providerSetupStage = 'credentials';
    state.apiTest = {
      status: target.value === 'browser-account' ? 'warning' : 'idle',
      message:
        target.value === 'browser-account'
          ? 'OpenAI/Codex authorization selected. Start and complete authorization before testing live chat.'
          : 'Authorization method changed. Test the provider before using live chat.',
    };
    render();
    return;
  }

  if (target.id === 'thinking-level') {
    state.thinkingLevel = target.value;
    notify('info', 'Thinking level changed', `Chat thinking level set to ${target.value}.`);
    render();
    return;
  }

  if (target.id === 'settings-provider') {
    updateProviderFieldsFromPreset(target.value);
    render();
    return;
  }

  if (target.id === 'settings-security') {
    state.securityProfile = target.value;
    render();
    return;
  }

  if (target.id === 'settings-runtime') {
    state.runtimeMode = target.value;
    render();
  }
}

async function handleClick(event) {
  const target = event.target;
  const element = target instanceof Element ? target : null;
  if (!element) return;

  const externalLink = element.closest('[data-open-external]');
  if (externalLink) {
    event.preventDefault();
    await openExternalUrl(externalLink.dataset.openExternal || externalLink.href);
    render();
    return;
  }

  if (element.closest('#notifications-button')) {
    toggleNotificationsMenu();
    render();
    return;
  }

  const navButton = element.closest('[data-section]');
  if (navButton) {
    setActiveSection(navButton.dataset.section);
    return;
  }

  const dismissButton = element.closest('[data-dismiss-notification]');
  if (dismissButton) {
    dismissNotification(dismissButton.dataset.dismissNotification);
    render();
    return;
  }

  if (element.closest('[data-toggle-notification-mute]')) {
    toggleNotificationsMuted();
    render();
    return;
  }

  if (element.closest('[data-clear-notifications]')) {
    clearNotifications();
    render();
    return;
  }

  if (element.closest('[data-restart-onboarding]')) {
    restartOnboarding();
    return;
  }

  if (element.closest('[data-cancel-onboarding]')) {
    cancelOnboarding();
    return;
  }

  const stepButton = element.closest('[data-onboarding-step]');
  if (stepButton) {
    state.onboardingStep = Number(stepButton.dataset.onboardingStep);
    render();
    return;
  }

  const experienceButton = element.closest('[data-experience-level]');
  if (experienceButton) {
    state.experienceLevel = experienceButton.dataset.experienceLevel;
    render();
    return;
  }

  const runtimeButton = element.closest('[data-runtime-mode]');
  if (runtimeButton) {
    state.runtimeMode = runtimeButton.dataset.runtimeMode;
    render();
    return;
  }

  const providerButton = element.closest('[data-provider-id]');
  if (providerButton) {
    updateProviderFieldsFromPreset(providerButton.dataset.providerId);
    render();
    return;
  }

  const providerAuthButton = element.closest('[data-provider-auth-method]');
  if (providerAuthButton) {
    state.providerAuthMethod = providerAuthButton.dataset.providerAuthMethod;
    ensureProviderModelAllowed();
    state.providerSetupStage = 'credentials';
    state.apiTest = {
      status: state.providerAuthMethod === 'browser-account' ? 'warning' : 'idle',
      message:
        state.providerAuthMethod === 'browser-account'
          ? 'OpenAI/Codex authorization selected. Start and complete authorization before testing provider access.'
          : 'API key authorization selected. Add the key, then choose a model.',
    };
    render();
    return;
  }

  const providerStageButton = element.closest('[data-provider-setup-stage]');
  if (providerStageButton) {
    const nextStage = providerStageButton.dataset.providerSetupStage;
    if (nextStage === 'model') {
      const provider = providerPresets.find((item) => item.id === state.llmProvider) || providerPresets[0];
      if (state.providerAuthMethod === 'api-key' && provider.requiresKey && !state.providerApiKey.trim()) {
        notify('error', 'API key needed', 'Add the API key first, or go back and choose browser account authorization.');
        render();
        return;
      }
      if (state.providerAuthMethod === 'browser-account' && state.codexOAuth.status !== 'ok') {
        notify('error', 'Authorization incomplete', 'Complete OpenAI/Codex authorization before choosing a model.');
        render();
        return;
      }
    }
    state.providerSetupStage = nextStage || 'provider';
    if (state.providerSetupStage === 'provider') state.providerSelectionConfirmed = false;
    if (state.providerSetupStage === 'model') ensureProviderModelAllowed();
    render();
    return;
  }

  const securityButton = element.closest('[data-security-profile]');
  if (securityButton) {
    state.securityProfile = securityButton.dataset.securityProfile;
    render();
    return;
  }

  if (element.closest('#choose-workspace')) {
    await chooseWorkspaceFolder();
    render();
    return;
  }

  if (element.closest('#test-provider')) {
    await testProvider();
    render();
    return;
  }

  if (element.closest('#continue-provider-model')) {
    const provider = providerPresets.find((item) => item.id === state.llmProvider) || providerPresets[0];
    if (provider.requiresKey && !state.providerApiKey.trim()) {
      notify('error', 'API key needed', 'Paste the provider API key before choosing the model.');
      render();
      return;
    }
    state.providerSetupStage = 'model';
    render();
    return;
  }

  if (element.closest('#attach-file')) {
    await chooseChatAttachment();
    render();
    return;
  }

  if (element.closest('#voice-input')) {
    startVoiceInput();
    render();
    return;
  }

  if (element.closest('#start-codex-oauth')) {
    state.providerAuthMethod = 'browser-account';
    ensureProviderModelAllowed();
    await startCodexOAuth();
    render();
    return;
  }

  if (element.closest('#complete-codex-oauth')) {
    state.providerAuthMethod = 'browser-account';
    ensureProviderModelAllowed();
    await completeCodexOAuth();
    render();
    return;
  }

  const recentChatButton = element.closest('[data-recent-chat]');
  if (recentChatButton) {
    setActiveChatSession(recentChatButton.dataset.recentChat);
    render();
    return;
  }

  if (element.closest('#new-chat')) {
    createChatSession();
    render();
    return;
  }

  if (element.closest('#back-button')) {
    state.onboardingStep = Math.max(1, state.onboardingStep - 1);
    render();
    return;
  }

  if (element.closest('#next-button')) {
    if (state.onboardingStep === onboardingSteps.length) await finishOnboarding();
    else advanceOnboarding();
    return;
  }

  const copyButton = element.closest('[data-copy-command]');
  if (copyButton) {
    await copyCommand(copyButton.dataset.copyCommand);
    return;
  }

  const actionButton = element.closest('[data-run-action]');
  if (actionButton) {
    await runAction(actionButton.dataset.runAction);
    return;
  }

  const chatActionButton = element.closest('[data-chat-action]');
  if (chatActionButton) {
    await runChatAction(chatActionButton.dataset.chatAction);
    return;
  }

  if (element.closest('[data-refresh-state]')) {
    await refreshDesktopState({ announce: true });
    render();
    return;
  }

  const approvalButton = element.closest('[data-approval]');
  if (approvalButton) {
    state.actionStatus = `Review opened for ${approvalButton.dataset.approval}.`;
    notify('info', 'Approval review', 'Detailed approval editing will open here when the broker is wired.');
    render();
    return;
  }

  if (element.closest('[data-repair-action]')) {
    notify(
      'warning',
      'Repair requires approval',
      'Argentum can suggest repairs, but it will not self-modify or run repair commands without explicit permission.',
    );
    render();
    return;
  }

  const chatOption = element.closest('[data-chat-option]');
  if (chatOption) {
    const optionId = chatOption.dataset.chatOption;
    if (optionId === 'gateway') {
      await runAction('gateway-start');
      return;
    }
    if (optionId === 'provider') {
      await runChatAction('test-provider');
      return;
    }
    if (optionId === 'security-policy') {
      setActiveSection('security');
      return;
    }
    if (optionId === 'profile') {
      notify('info', 'Profile panel', 'Use the profile fields on the right side of Chat to set your name and the agent name.');
      render();
      return;
    }

    state.chatBlocks.push({
      type: 'summary',
      title: 'Selected',
      body: `You chose ${chatOption.textContent.trim()}.`,
    });
    render();
    return;
  }

  if (element.closest('#save-profile')) {
    saveProfileFromInputs();
    render();
    return;
  }

  if (element.closest('#send-chat')) {
    await sendChatDraft();
  }
}

document.querySelector('#audit-button').addEventListener('click', () => setActiveSection('security'));
document.querySelector('#settings-button').addEventListener('click', () => setActiveSection('settings'));
document.addEventListener('click', handleClick);
document.addEventListener('input', handleInput);
document.addEventListener('change', handleChange);
window.addEventListener('argentum:state-change', render);

hydrateStaticIcons(document);
const chatHistoryRestored = hydrateChatHistory();
scheduleVisibleNotifications();
hydrateDesktopDefaults()
  .then(() => refreshDesktopState())
  .then(() => {
    if (state.desktopState?.configExists) {
      state.setupComplete = true;
      state.onboardingOpen = false;
      state.activeSection = 'chat';
      state.notifications = [];
      if (!chatHistoryRestored) resetIntroChat();
    } else {
      state.onboardingOpen = true;
    }
  })
  .finally(() => render());
