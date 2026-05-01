import { providerPresets, sections } from './modules/constants.js';
import { hydrateStaticIcons } from './modules/icons.js';
import { modules } from './modules/sections.js';
import { renderModule, renderNavigation } from './modules/shell.js';
import {
  appendChatMessage,
  dismissNotification,
  notify,
  setChannel,
  setProvider,
  state,
} from './modules/state.js';
import {
  chooseWorkspaceFolder,
  refreshDesktopState,
  saveSetup,
  testProvider,
  hydrateDesktopDefaults,
} from './modules/setup.js';
import { isProbablyAbsolutePath, normalizeError } from './modules/utils.js';

const nav = document.querySelector('#section-nav');
const title = document.querySelector('#section-title');
const eyebrow = document.querySelector('#eyebrow');
const viewRoot = document.querySelector('#view-root');
const workspacePath = document.querySelector('#workspace-path');

function activeSection() {
  return sections.find((section) => section.id === state.activeSection) || sections[0];
}

function render() {
  const section = activeSection();
  const module = modules[section.id] || modules.chat;

  workspacePath.textContent = state.workspacePath;
  title.textContent = section.title;
  eyebrow.textContent = section.eyebrow;
  nav.innerHTML = renderNavigation();
  viewRoot.innerHTML = renderModule(module);
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

  state.onboardingStep = Math.min(9, state.onboardingStep + 1);
  render();
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
    state.onboardingStep = 9;
    state.chatBlocks = [
      {
        type: 'message',
        role: 'argentum',
        title: 'Argentum',
        body: 'Setup is saved. What should I call you in this workspace?',
      },
      {
        type: 'optionGroup',
        title: 'Next setup choice',
        body: 'After your name, I can help tune features and security policies.',
        options: [
          { id: 'name-agent', label: 'Name the agent', detail: 'Set how Argentum should introduce itself.' },
          { id: 'security-policy', label: 'Security policy', detail: 'Choose how strict approvals should be.' },
          { id: 'features', label: 'Feature set', detail: 'Pick skills, memory, and channels.' },
        ],
      },
    ];
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
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    state.actionStatus = `Prepared action: ${actionId}`;
    notify('info', 'Action prepared', 'Run the installed app to execute desktop actions through Tauri.');
    render();
    return;
  }

  try {
    const result = await invoke('run_desktop_action', {
      request: { actionId, workspacePath: state.workspacePath },
    });
    state.actionStatus = result?.message || `${actionId} prepared.`;
    notify('success', 'Action prepared', state.actionStatus);
    await refreshDesktopState();
  } catch (error) {
    notify('error', 'Action failed', normalizeError(error));
  }
  render();
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
}

function handleInput(event) {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;

  if (target.id === 'workspace-input' || target.id === 'settings-workspace') {
    state.workspacePath = target.value;
    workspacePath.textContent = state.workspacePath;
    return;
  }

  if (target.id === 'provider-base-url') state.providerBaseUrl = target.value;
  if (target.id === 'provider-model') state.providerModel = target.value;
  if (target.id === 'provider-api-key') state.providerApiKey = target.value;
  if (target.id === 'custom-provider-name') state.customProviderName = target.value;
  if (target.id === 'custom-api-key-env') state.customApiKeyEnv = target.value;
  if (target.id === 'webchat-token') state.webchatToken = target.value;
  if (target.id === 'telegram-token') state.telegramToken = target.value;
  if (target.id === 'telegram-allowlist') state.telegramAllowlist = target.value;
  if (target.id === 'whatsapp-phone-id') state.whatsappPhoneId = target.value;
  if (target.id === 'chat-draft') state.draftMessage = target.value;
}

function handleChange(event) {
  const target = event.target;
  if (!(target instanceof HTMLInputElement) && !(target instanceof HTMLSelectElement)) return;

  if (target.dataset.channelId) {
    setChannel(target.dataset.channelId, target.checked);
    render();
    return;
  }

  if (target.id === 'provider-api') {
    state.providerApi = target.value;
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

  const stepButton = element.closest('[data-onboarding-step]');
  if (stepButton) {
    state.onboardingStep = Number(stepButton.dataset.onboardingStep);
    state.activeSection = 'onboarding';
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

  if (element.closest('#back-button')) {
    state.onboardingStep = Math.max(1, state.onboardingStep - 1);
    render();
    return;
  }

  if (element.closest('#next-button')) {
    if (state.onboardingStep === 9) await finishOnboarding();
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
    state.chatBlocks.push({
      type: 'summary',
      title: 'Selected',
      body: `You chose ${chatOption.textContent.trim()}. Argentum will turn this into a guided setup card next.`,
    });
    render();
    return;
  }

  if (element.closest('#send-chat')) {
    const draft = state.draftMessage.trim();
    if (!draft) return;

    appendChatMessage('user', draft);
    appendChatMessage(
      'argentum',
      'Got it. I am keeping this local for now. Once provider testing passes, this same chat surface can switch to live model execution.',
    );
    state.draftMessage = '';
    render();
  }
}

document.querySelector('#audit-button').addEventListener('click', () => setActiveSection('security'));
document.querySelector('#settings-button').addEventListener('click', () => setActiveSection('settings'));
document.addEventListener('click', handleClick);
document.addEventListener('input', handleInput);
document.addEventListener('change', handleChange);

hydrateStaticIcons(document);
hydrateDesktopDefaults()
  .then(() => refreshDesktopState())
  .finally(() => render());
