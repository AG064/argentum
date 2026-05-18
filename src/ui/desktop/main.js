import { commandCatalog, modelMetadata, onboardingSteps, sections } from './modules/constants.js';
import { hydrateStaticIcons } from './modules/icons.js';
import { modules } from './modules/sections.js';
import {
  clearOnboardingError,
  clearOnboardingProgress,
  completeOnboarding,
  goToStep,
  hydrateOnboardingProgress,
  nextStep,
  previousStep,
  restartOnboardingState,
  selectAuthMethod,
  selectExperienceLevel,
  selectModel,
  selectProvider,
  setProviderSetupStage,
  validateCurrentStep,
} from './modules/onboarding-controller.js';
import { renderModule, renderNavigation, renderProviderStatusPill } from './modules/shell.js';
import {
  addTerminalEntry,
  appendChatMessage,
  cancelDeleteChatSession,
  clearChatSession,
  clearNotifications,
  compactActiveChatSession,
  confirmDeleteChatSession,
  createChatSession,
  dismissNotification,
  hydrateChatHistory,
  hydrateUiPreferences,
  notify,
  recordUiEvent,
  renameChatSession,
  scheduleVisibleNotifications,
  setChannel,
  setActiveChatSession,
  setChatFilter,
  setOnboardingDisclosure,
  setSettingsSection,
  setUiPreference,
  setViewMode,
  state,
  syncActiveChatSession,
  requestDeleteChatSession,
  toggleChatPinned,
  toggleConversationMenu,
  toggleHelp,
  toggleNotificationsMenu,
  toggleNotificationsMuted,
  toggleQuickSecurityMenu,
  toggleQuickSettingsMenu,
  toggleWorkspaceMenu,
  updateChatMessage,
} from './modules/state.js';
import {
  chooseWorkspaceFolder,
  completeCodexOAuth,
  openExternalUrl,
  persistRuntimeSettings,
  refreshDesktopState,
  saveSetup,
  sendChatMessage,
  startCodexOAuth,
  testProvider,
  hydrateDesktopDefaults,
} from './modules/setup.js';
import {
  filePreviewUrl,
  escapeHtml,
  inferAttachmentKind,
  inferMimeType,
  isProbablyAbsolutePath,
  contextUsagePercent,
  estimateRuntimeContextTokens,
  modelMetadataFor,
  modelSupportsVision,
  normalizeError,
  openFile,
} from './modules/utils.js';

const nav = document.querySelector('#section-nav');
const title = document.querySelector('#section-title');
const eyebrow = document.querySelector('#eyebrow');
const viewRoot = document.querySelector('#view-root');
const overlayRoot = document.querySelector('#overlay-root');
const workspacePath = document.querySelector('#workspace-path');
const providerStatusPill = document.querySelector('#provider-status-pill');
const viewModeButtons = [...document.querySelectorAll('[data-view-mode]')];

const chatActions = {
  'test-provider': { label: 'Test Provider' },
  'gateway-start': { actionId: 'gateway-start', label: 'Start Gateway' },
  'gateway-status': { actionId: 'gateway-status', label: 'Check Gateway' },
  'settings': { section: 'settings', label: 'Open Settings' },
};

const onboardingKeyboardActivationEvents = new Set(['keyup']);
const ACTIVATION_HANDLED_FLAG = '__argentumActivationHandled';

function applyUiPreferences() {
  document.documentElement.style.setProperty('--font-ui', state.uiFontFamily);
  document.documentElement.style.setProperty('--font-mono', state.codeFontFamily);
}

function eventTargetElement(event) {
  const target = event.target;
  if (target instanceof Element) return target;

  const currentTarget = event.currentTarget;
  if (currentTarget instanceof Element) return currentTarget;

  const path = typeof event.composedPath === 'function' ? event.composedPath() : [];
  const pathElement = path.find((item) => item instanceof Element);
  if (pathElement) return pathElement;

  const parentElement = target?.parentElement;
  if (parentElement instanceof Element) return parentElement;

  const pointElement =
    Number.isFinite(event.clientX) && Number.isFinite(event.clientY)
      ? document.elementFromPoint(event.clientX, event.clientY)
      : null;
  return pointElement instanceof Element ? pointElement : null;
}

function isOnboardingActivation(element) {
  if (!element.closest('.onboarding-modal')) return false;

  return Boolean(
    element.closest(
      [
        '#next-button',
        '#back-button',
        '#choose-workspace',
        '#continue-provider-model',
        '#test-provider',
        '#start-codex-oauth',
        '#complete-codex-oauth',
        '[data-experience-level]',
        '[data-onboarding-step]',
        '[data-provider-id]',
        '[data-provider-auth-method]',
        '[data-provider-catalog-tab]',
        '[data-runtime-mode]',
        '[data-security-profile]',
        '[data-cancel-onboarding]',
      ].join(', '),
    ),
  );
}

async function handleActivation(event) {
  if (event[ACTIVATION_HANDLED_FLAG]) return;

  const element = eventTargetElement(event);
  if (!element) return;
  if (window.location.search.includes('debugEvents=1')) {
    console.debug(
      '[Argentum activation]',
      JSON.stringify({
        type: event.type,
        target: element.tagName,
        id: element.id,
        classes: element.getAttribute('class'),
        experienceLevel:
          element.closest('[data-experience-level]')?.getAttribute('data-experience-level') || '',
        next: Boolean(element.closest('#next-button')),
        text: element.textContent?.trim().slice(0, 80),
      }),
    );
  }

  if (onboardingKeyboardActivationEvents.has(event.type)) {
    const isKeyboardActivation = event.key === 'Enter' || event.key === ' ';
    if (event.type === 'keyup' && !isKeyboardActivation) return;
    if (!isOnboardingActivation(element)) return;
    event[ACTIVATION_HANDLED_FLAG] = true;
    event.preventDefault();
    try {
      await handleClick(event, element);
    } catch (error) {
      console.error('[Argentum activation failed]', error);
    }
    return;
  }

  event[ACTIVATION_HANDLED_FLAG] = true;
  try {
    await handleClick(event, element);
  } catch (error) {
    console.error('[Argentum activation failed]', error);
  }
}

function activeSection() {
  return (
    sections.find((section) => section.id === state.activeSection && section.id !== 'onboarding') ||
    sections[1]
  );
}

function render() {
  const chatScroll = captureChatTranscriptScroll();
  applyUiPreferences();
  const section = activeSection();
  const module = modules[section.id] || modules.chat;

  workspacePath.textContent = state.workspacePath;
  title.textContent = section.title;
  eyebrow.textContent = section.eyebrow;
  nav.innerHTML = renderNavigation();
  providerStatusPill.innerHTML = renderProviderStatusPill();
  viewModeButtons.forEach((button) => {
    button.classList.toggle('active', button.dataset.viewMode === state.viewMode);
    button.setAttribute('aria-pressed', String(button.dataset.viewMode === state.viewMode));
    button.disabled = section.id !== 'chat';
  });
  document.querySelector('.view-mode-switcher')?.classList.toggle('hidden', section.id !== 'chat');
  document.body.dataset.activeViewMode = state.viewMode;
  viewRoot.className = `view-root view-root-${section.id} view-mode-${state.viewMode}`;
  viewRoot.innerHTML = renderModule(module);
  const overlayHtml = `${renderFloatingPanels(section)}${state.onboardingOpen ? renderModule(modules.onboarding) : ''}`;
  if (overlayRoot) {
    overlayRoot.innerHTML = overlayHtml;
  } else {
    viewRoot.insertAdjacentHTML('beforeend', overlayHtml);
  }
  hydrateStaticIcons(document);
  wireRenderedControls();
  syncSystemDashboardFrame();
  syncComposerSendState();
  scrollTerminalPanels();
  restoreChatTranscriptScroll(chatScroll);
}

function renderFloatingPanels(section) {
  return `${renderHelpPanel(section)}${renderNotificationMenu()}${renderQuickSecurityPanel()}${renderQuickSettingsPanel()}${renderWorkspacePanel()}`;
}

function captureChatTranscriptScroll() {
  const panel = document.querySelector('.chat-transcript');
  if (!(panel instanceof HTMLElement)) return null;
  return {
    top: panel.scrollTop,
    height: panel.scrollHeight,
    pinned: panel.scrollTop + panel.clientHeight >= panel.scrollHeight - 96,
  };
}

function restoreChatTranscriptScroll(previous) {
  const schedule = window.requestAnimationFrame || ((callback) => window.setTimeout(callback, 0));
  schedule(() => {
    const panel = document.querySelector('.chat-transcript');
    if (!(panel instanceof HTMLElement)) return;

    if (state.chatScrollIntent === 'bottom') {
      panel.scrollTop = panel.scrollHeight;
      state.chatScrollIntent = '';
      return;
    }

    if (!previous) return;
    if (previous.pinned) {
      panel.scrollTop = panel.scrollHeight;
      return;
    }

    const heightDelta = panel.scrollHeight - previous.height;
    panel.scrollTop = Math.max(0, previous.top + Math.max(0, heightDelta));
  });
}

function renderHelpPanel(section) {
  if (!state.helpOpen) return '';

  const tips = {
    chat: [
      [
        'View modes',
        'Chat only hides optional panels. Split view adds the inspector. Full workspace shows the widest context.',
      ],
      [
        'Context',
        'The green ring near Send estimates how much model context the current session uses.',
      ],
      [
        'Reasoning',
        'Reasoning blocks stay collapsed and hidden unless you enable them in Settings.',
      ],
    ],
    gateway: [
      [
        'Gateway',
        'The gateway is the local service entrance. Start, stop, status, and logs run fixed whitelisted commands.',
      ],
      ['Logs', 'Gateway output is redacted before it is shown in the app.'],
    ],
    settings: [
      [
        'Sections',
        'Open one settings section at a time from the left. Advanced details stay tucked away.',
      ],
      [
        'Provider test',
        'Testing validates the saved key or account flow before live chat uses it.',
      ],
    ],
  };
  const sectionTips = tips[section.id] || [
    [
      'Current screen',
      'Use the left rail for main areas and the top-right buttons for current-view help and status.',
    ],
    [
      'Safety',
      'Argentum keeps privileged runtime actions behind fixed commands and workspace validation.',
    ],
  ];

  return `
    <aside class="help-panel floating-panel" role="dialog" aria-label="Help">
      <div class="split-header">
        <div>
          <span class="pill">Help</span>
          <h3>${escapeHtml(section.title)}</h3>
        </div>
        <button class="icon-button compact" data-close-help="true" aria-label="Close help"><span data-icon="x"></span></button>
      </div>
      <div class="help-callout-list">
        ${sectionTips
          .map(
            ([title, detail]) => `
              <article>
                <strong>${escapeHtml(title)}</strong>
                <p>${escapeHtml(detail)}</p>
              </article>
            `,
          )
          .join('')}
      </div>
    </aside>
  `;
}

function renderWorkspacePanel() {
  if (!state.workspaceMenuOpen) return '';

  return `
    <aside class="workspace-menu floating-panel" role="menu" aria-label="Workspace menu">
      <div class="split-header">
        <div>
          <span class="pill">Workspace</span>
          <h3>Current workspace</h3>
        </div>
        <button class="icon-button compact" data-close-workspace-menu="true" aria-label="Close workspace menu"><span data-icon="x"></span></button>
      </div>
      <div class="workspace-menu-body">
        <div>
          <span>Path</span>
          <strong>${escapeHtml(state.workspacePath)}</strong>
        </div>
        <button class="button" data-section="settings" data-close-workspace-menu="true">Workspace settings</button>
        <button class="button" id="choose-workspace">Choose folder</button>
        <button class="button" disabled title="Multiple workspaces are planned and documented, but not enabled in this MVP.">Switch workspace</button>
      </div>
    </aside>
  `;
}

function setActiveSection(sectionId) {
  if (!modules[sectionId]) return;
  state.activeSection = sectionId;
  render();
}

function advanceOnboarding() {
  const result = nextStep();
  if (!result.ok) {
    notify('error', 'Setup needs a fix', result.message);
    render();
    return;
  }
  render();
}

function restartOnboarding() {
  restartOnboardingState();
  notify(
    'info',
    'Onboarding opened',
    'The setup dialog is back on top. Finish or cancel to return to the app.',
  );
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
  const validation = validateCurrentStep();
  if (!validation.valid) {
    state.onboardingError = validation.message;
    state.onboardingValidationErrors = validation.errors;
    notify('error', 'Setup needs a fix', validation.message);
    render();
    return;
  }

  if (!isProbablyAbsolutePath(state.workspacePath)) {
    notify(
      'error',
      'Choose a full folder path',
      'Workspace path must be absolute. Use Browse and choose a folder like C:\\Users\\You\\Argentum Workspace.',
    );
    goToStep(2);
    render();
    return;
  }

  state.setupAnimation = true;
  render();

  try {
    const result = await saveSetup();
    completeOnboarding(result);
    resetIntroChat();
    addTerminalEntry(
      'argentum setup save',
      `Configuration saved${state.savedConfigPath ? ` to ${state.savedConfigPath}` : ' in the selected workspace'}.`,
      'success',
    );
    notify(
      'success',
      'Setup complete',
      'Onboarding is hidden. Chat is ready for the introductory phase.',
    );
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
  addTerminalEntry(command, 'Running...', 'info');
  render();

  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    state.actionStatus = `Preview mode: ${command} needs the installed desktop app.`;
    addTerminalEntry(
      command,
      'Preview mode: the installed Tauri app is required to execute gateway actions.',
      'warning',
    );
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
    notify(
      status === 'stopped' ? 'info' : 'success',
      action?.title || 'Desktop action',
      state.actionStatus,
    );
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
  return selectProvider(providerId);
}

function renderNotificationMenu() {
  if (!state.notificationsMenuOpen) return '';

  const history = state.notificationHistory.slice(0, 14);
  return `
    <aside class="notification-menu floating-panel modern-menu" role="dialog" aria-label="Notifications">
      <div class="split-header">
        <div>
          <span class="pill">Notifications</span>
          <h3>Activity center</h3>
        </div>
        <button class="icon-button compact" data-close-notifications="true" aria-label="Close notifications"><span data-icon="x"></span></button>
      </div>
      <div class="notification-toolbar">
        <button class="button" data-toggle-notification-mute="true">${state.notificationsMuted ? 'Unmute' : 'Mute'}</button>
        <button class="button" data-clear-notifications="true">Clear</button>
      </div>
      <div class="notification-history modern-notification-history">
        ${
          history.length === 0
            ? '<article class="notification-history-item info"><strong>No notifications</strong><p>History is clear.</p></article>'
            : history
                .map(
                  (notification) => `
                    <article class="notification-history-item ${escapeHtml(notification.type || 'info')}">
                      <span></span>
                      <div>
                        <strong>${escapeHtml(notification.title)}</strong>
                        <p>${escapeHtml(notification.message)}</p>
                      </div>
                    </article>
                  `,
                )
                .join('')
        }
      </div>
    </aside>
  `;
}

function renderQuickSecurityPanel() {
  if (!state.quickSecurityMenuOpen) return '';

  const profiles = [
    ['restricted', 'Restricted', 'Workspace only'],
    ['ask', 'Ask', 'Prompt for privileged actions'],
    ['session', 'Session', 'Temporary grants'],
    ['trusted', 'Trusted', 'Fewer prompts, audited'],
  ];

  return `
    <aside class="quick-menu floating-panel modern-menu" role="dialog" aria-label="Quick security">
      <div class="split-header">
        <div>
          <span class="pill">Security</span>
          <h3>Current view controls</h3>
        </div>
        <button class="icon-button compact" data-close-quick-menu="security" aria-label="Close security menu"><span data-icon="x"></span></button>
      </div>
      <div class="quick-menu-grid">
        ${profiles
          .map(
            ([id, label, detail]) => `
              <button class="quick-choice ${state.securityProfile === id ? 'active' : ''}" data-security-profile="${id}">
                <strong>${label}</strong>
                <span>${detail}</span>
              </button>
            `,
          )
          .join('')}
      </div>
      <p class="muted-line">Changes affect the active workspace profile and remain permission-gated by the desktop bridge.</p>
    </aside>
  `;
}

function renderQuickSettingsPanel() {
  if (!state.quickSettingsMenuOpen) return '';

  return `
    <aside class="quick-menu floating-panel modern-menu" role="dialog" aria-label="Quick settings">
      <div class="split-header">
        <div>
          <span class="pill">Quick settings</span>
          <h3>${escapeHtml(activeSection().title)}</h3>
        </div>
        <button class="icon-button compact" data-close-quick-menu="settings" aria-label="Close quick settings"><span data-icon="x"></span></button>
      </div>
      <div class="quick-setting-list">
        <label>
          <span>Thinking</span>
          <select id="thinking-level">
            <option value="fast" ${state.thinkingLevel === 'fast' ? 'selected' : ''}>Fast</option>
            <option value="balanced" ${state.thinkingLevel === 'balanced' ? 'selected' : ''}>Balanced</option>
            <option value="deep" ${state.thinkingLevel === 'deep' ? 'selected' : ''}>Deep</option>
          </select>
        </label>
        <button class="button" data-compact-context="true">Compact current chat</button>
        <button class="button" data-section="settings" data-close-quick-menu="settings">Open full settings</button>
        <button class="button" data-restart-onboarding="true">Restart onboarding</button>
      </div>
    </aside>
  `;
}

function syncComposerSendState() {
  const sendButton = document.querySelector('[data-send-chat-button]');
  const status = document.querySelector('#send-chat-status');
  const hasDraft = state.draftMessage.trim().length > 0;
  const hasAttachments = state.chatAttachments.length > 0;
  const disabled = state.chatStreaming || (!hasDraft && !hasAttachments);
  const message = state.chatStreaming
    ? 'Generation in progress. Stop it before sending another message.'
    : disabled
      ? 'Type a message or attach a file to send.'
      : 'Ready to send.';

  if (sendButton instanceof HTMLButtonElement) {
    sendButton.disabled = disabled;
    sendButton.setAttribute('aria-disabled', String(disabled));
    sendButton.title = message;
  }
  if (status instanceof HTMLElement) {
    status.textContent = message;
  }
}

function addActivationListeners(target) {
  if (!target) return;
  target.addEventListener('click', handleActivation, true);
  target.addEventListener('click', handleActivation);
  for (const activationEvent of onboardingKeyboardActivationEvents) {
    target.addEventListener(activationEvent, handleActivation, true);
  }
}

function wireRenderedControls() {
  for (const root of [viewRoot, overlayRoot]) {
    if (!root) continue;
    root.querySelectorAll('button, a[data-open-external]').forEach((control) => {
      if (!(control instanceof Element)) return;
      if (control.getAttribute('data-argentum-click-wired') === 'true') return;
      control.setAttribute('data-argentum-click-wired', 'true');
      control.addEventListener('click', handleActivation);
    });
  }

  const dashboardFrame = document.querySelector('[data-system-dashboard-frame]');
  if (
    dashboardFrame instanceof HTMLIFrameElement &&
    dashboardFrame.dataset.argentumFrameWired !== 'true'
  ) {
    dashboardFrame.dataset.argentumFrameWired = 'true';
    dashboardFrame.addEventListener('load', syncSystemDashboardFrame);
  }
}

function syncSystemDashboardFrame() {
  const dashboardFrame = document.querySelector('[data-system-dashboard-frame]');
  if (!(dashboardFrame instanceof HTMLIFrameElement) || !dashboardFrame.contentWindow) return;
  dashboardFrame.contentWindow.postMessage(
    {
      type: 'argentum-system-stats',
      stats: state.desktopState?.systemStats || null,
    },
    '*',
  );
}

function scrollTerminalPanels() {
  const schedule = window.requestAnimationFrame || ((callback) => window.setTimeout(callback, 0));
  schedule(() => {
    document.querySelectorAll('.terminal-body').forEach((panel) => {
      panel.scrollTop = panel.scrollHeight;
    });
  });
}

function applyRuntimeMode(reason = 'runtime-mode') {
  const runtimeLabel =
    state.runtimeMode === 'cli'
      ? 'CLI tools selected. Desktop settings still save to the workspace config used by argentum commands.'
      : state.runtimeMode === 'service'
        ? 'Local service mode selected. Gateway and integration controls stay permission-gated.'
        : 'Desktop app mode selected. Chat, settings, diagnostics, and gateway controls remain in the GUI.';

  state.actionStatus = runtimeLabel;
  addTerminalEntry(`argentum runtime ${state.runtimeMode}`, runtimeLabel, 'info');
  notify('info', 'Runtime mode changed', runtimeLabel);

  if (!state.setupComplete) return Promise.resolve(null);
  return persistRuntimeSettings(reason, { notify: false }).catch((error) => {
    notify('error', 'Runtime mode was not saved', normalizeError(error));
  });
}

async function chooseChatAttachment() {
  const selected = await openFile(state.workspacePath);
  if (!selected) {
    notify('info', 'No file attached', 'No file was selected.');
    return;
  }

  const path = Array.isArray(selected) ? selected[0] : selected;
  const mime = inferMimeType(path);
  const kind = inferAttachmentKind(path, mime);
  state.chatAttachments = [
    ...state.chatAttachments,
    {
      id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
      path,
      name: String(path).split(/[\\/]/).pop() || String(path),
      mime,
      kind,
      previewSrc: kind === 'image' ? filePreviewUrl(path) : '',
    },
  ].slice(-6);
  notify(
    'success',
    kind === 'image' ? 'Image attached' : 'File attached',
    'The selected attachment will be sent with the next chat message.',
  );
}

function startVoiceInput() {
  const Recognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!Recognition) {
    state.voiceInputStatus = 'unsupported';
    notify(
      'error',
      'Microphone unavailable',
      'Voice dictation is not available in this desktop webview yet.',
    );
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
    state.draftMessage =
      `${state.draftMessage}${state.draftMessage ? ' ' : ''}${transcript}`.trim();
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

async function saveSettingsFromInputs() {
  const userInput = document.querySelector('#profile-user-name');
  const agentInput = document.querySelector('#profile-agent-name');
  const purposeInput = document.querySelector('#profile-purpose');
  const keyInput = document.querySelector('#settings-provider-api-key');

  if (userInput instanceof HTMLInputElement) state.userName = userInput.value.trim();
  if (agentInput instanceof HTMLInputElement)
    state.agentName = agentInput.value.trim() || 'Argentum';
  if (purposeInput instanceof HTMLTextAreaElement) state.systemPrompt = purposeInput.value.trim();
  if (keyInput instanceof HTMLInputElement) state.providerApiKey = keyInput.value.trim();

  try {
    await persistRuntimeSettings('settings', { notify: true });
    state.providerApiKey = '';
    appendChatMessage(
      'argentum',
      `Settings saved. I will use **${state.agentName || 'Argentum'}** as the agent name${state.userName ? ` and call you **${state.userName}**` : ''}.`,
    );
  } catch (error) {
    notify('error', 'Settings could not be saved', normalizeError(error));
  }
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
    return `Security status: ${state.securityProfile}. Default file access is only inside ${state.workspacePath}. Approved model tools may read/write workspace files and fetch localhost endpoints; external folders, arbitrary shell, external network, RAM, browser sessions, and OS control remain blocked unless a future permission-gated feature is approved.`;
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

function attachmentToPayload(file) {
  return {
    id: file.id,
    path: file.path,
    name: file.name,
    mime: file.mime || inferMimeType(file.path),
    kind: file.kind || inferAttachmentKind(file.path, file.mime),
  };
}

function sleep(ms) {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

async function streamAssistantMessage(text, options = {}) {
  const block = options.targetId
    ? updateChatMessage(options.targetId, { rawBody: '', status: 'streaming', error: '' })
    : appendChatMessage('argentum', '', {
        status: 'streaming',
        id: options.id,
      });
  if (!block) return null;
  state.activeAssistantMessageId = block.id;
  state.chatStreaming = true;
  state.chatAbortRequested = false;
  recordUiEvent('chat.response_started', 'running', 'Assistant response streaming started.', {
    chatId: state.activeChatId,
    messageId: block.id,
  });
  state.chatScrollIntent = 'bottom';
  render();

  const source = String(text || '').trim() || 'I did not receive a usable response.';
  let rawBody = '';
  const chunks = source.match(/.{1,18}(\s|$)|\S+/g) || [source];
  for (const chunk of chunks) {
    if (state.chatAbortRequested) {
      updateChatMessage(block.id, {
        rawBody: `${rawBody.trim()}\n\n[stopped]`,
        status: 'stopped',
      });
      recordUiEvent(
        'chat.response_cancelled',
        'stopped',
        'Assistant response was stopped by the user.',
        {
          chatId: state.activeChatId,
          messageId: block.id,
        },
      );
      break;
    }
    rawBody += chunk;
    updateChatMessage(block.id, {
      rawBody,
      status: 'streaming',
    });
    state.chatScrollIntent = 'bottom';
    render();
    await sleep(options.delay || 18);
  }

  if (!state.chatAbortRequested) {
    updateChatMessage(block.id, {
      rawBody: source,
      status: options.status || 'sent',
    });
    recordUiEvent('chat.response_completed', 'ok', 'Assistant response streaming completed.', {
      chatId: state.activeChatId,
      messageId: block.id,
    });
  }
  state.chatStreaming = false;
  state.activeAssistantMessageId = '';
  state.chatAbortRequested = false;
  state.chatScrollIntent = 'bottom';
  render();
  return block;
}

async function regenerateAssistantResponse(messageId) {
  if (state.chatStreaming) {
    notify('info', 'Generation already running', 'Stop the current response before regenerating.');
    return;
  }

  const targetIndex = state.chatBlocks.findIndex((block) => block.id === messageId);
  const target = state.chatBlocks[targetIndex];
  const lastUserMessage = [...state.chatBlocks.slice(0, targetIndex)]
    .reverse()
    .find((block) => block.type === 'message' && block.role === 'user');
  if (!target || !lastUserMessage?.body) {
    notify(
      'info',
      'Nothing to regenerate',
      'Regenerate needs an assistant message with a user message before it.',
    );
    render();
    return;
  }

  const outgoingMessage = lastUserMessage.rawBody || lastUserMessage.body;
  const attachments = (lastUserMessage.attachments || []).map(attachmentToPayload);
  state.chatStreaming = true;
  state.activeAssistantMessageId = messageId;
  state.chatAbortRequested = false;
  state.actionStatus = 'Regenerating response...';
  updateChatMessage(messageId, { rawBody: '', status: 'streaming', error: '' });
  recordUiEvent('chat.regenerate_started', 'running', 'Regenerating assistant response.', {
    chatId: state.activeChatId,
    messageId,
  });
  render();

  try {
    if (!state.setupComplete || !window.__TAURI__?.core?.invoke) {
      await streamAssistantMessage(buildLocalReply(outgoingMessage), { targetId: messageId });
      recordUiEvent(
        'chat.regenerate_completed',
        'ok',
        'Regeneration completed in local guided mode.',
        {
          chatId: state.activeChatId,
          messageId,
        },
      );
      return;
    }

    const result = await sendChatMessage(outgoingMessage, attachments);
    if (result?.usage) state.usageSnapshot = result.usage;
    await streamAssistantMessage(result?.message || buildLocalReply(outgoingMessage), {
      targetId: messageId,
    });
    recordUiEvent(
      'chat.regenerate_completed',
      'ok',
      'Regeneration completed with the configured provider.',
      {
        chatId: state.activeChatId,
        messageId,
        provider: result?.provider || state.llmProvider,
        model: result?.model || state.providerModel,
      },
    );
  } catch (error) {
    const message = normalizeError(error);
    updateChatMessage(messageId, {
      rawBody: `Live chat failed: ${message}`,
      status: 'error',
      error: message,
    });
    recordUiEvent('chat.regenerate_failed', 'error', message, {
      chatId: state.activeChatId,
      messageId,
    });
    notify('error', 'Regeneration failed', message);
  } finally {
    state.actionStatus = 'Chat is ready.';
    state.chatStreaming = false;
    state.activeAssistantMessageId = '';
    state.chatAbortRequested = false;
    recordUiEvent('chat.regenerate_state_reset', 'ok', 'Regeneration busy state reset.', {
      chatId: state.activeChatId,
      messageId,
    });
    render();
  }
}

async function sendChatDraft() {
  recordUiEvent('chat.send_attempt', 'running', 'Chat send requested from the desktop composer.', {
    chatId: state.activeChatId,
    hasDraft: state.draftMessage.trim().length > 0,
    attachmentCount: state.chatAttachments.length,
  });

  if (state.chatStreaming) {
    recordUiEvent(
      'chat.send_failed',
      'blocked',
      'Send blocked because a response is already streaming.',
      {
        chatId: state.activeChatId,
      },
    );
    notify(
      'info',
      'Generation in progress',
      'Stop the current response before sending another message.',
    );
    return;
  }

  const draft = state.draftMessage.trim();
  if (!draft && state.chatAttachments.length === 0) {
    recordUiEvent('chat.send_failed', 'blocked', 'Send blocked because the composer is empty.', {
      chatId: state.activeChatId,
    });
    syncComposerSendState();
    return;
  }

  const attachments = state.chatAttachments.map(attachmentToPayload);
  const hasImageAttachment = attachments.some(
    (file) => file.kind === 'image' || String(file.mime || '').startsWith('image/'),
  );
  if (hasImageAttachment && !modelSupportsVision(state.providerModel, modelMetadata)) {
    recordUiEvent(
      'chat.send_failed',
      'blocked',
      'Send blocked because the selected model cannot receive image attachments.',
      {
        chatId: state.activeChatId,
        model: state.providerModel,
      },
    );
    notify(
      'error',
      'Model cannot see images',
      `${state.providerModel} is not marked as vision-capable. Choose a vision model before sending images.`,
    );
    render();
    return;
  }

  const outgoingMessage = draft || 'Please analyze the attached file.';
  const metadata = modelMetadataFor(state.providerModel, modelMetadata);
  const estimatedContextPercent = contextUsagePercent(
    estimateRuntimeContextTokens(state, outgoingMessage),
    metadata,
  );
  if (estimatedContextPercent >= 85 && compactActiveChatSession({ automatic: true })) {
    notify(
      'info',
      'Context compacted',
      'Argentum summarized older messages before sending because the context window is getting full.',
    );
  }
  appendChatMessage('user', outgoingMessage, { attachments: state.chatAttachments });
  recordUiEvent('chat.message_sent', 'ok', 'User message added to the active chat.', {
    chatId: state.activeChatId,
    hasAttachments: state.chatAttachments.length > 0,
    attachmentCount: state.chatAttachments.length,
  });
  state.draftMessage = '';
  state.chatAttachments = [];
  state.chatScrollIntent = 'bottom';
  render();

  if (!state.setupComplete || !window.__TAURI__?.core?.invoke) {
    await streamAssistantMessage(buildLocalReply(draft || outgoingMessage));
    recordUiEvent('chat.send_success', 'offline', 'Chat message answered in local guided mode.', {
      chatId: state.activeChatId,
    });
    render();
    return;
  }

  state.actionStatus = 'Sending chat message...';
  state.chatStreaming = true;
  state.chatScrollIntent = 'bottom';
  render();

  try {
    const result = await sendChatMessage(outgoingMessage, attachments);
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
    if (result?.usage) state.usageSnapshot = result.usage;

    await streamAssistantMessage(result?.message || buildLocalReply(draft));
    recordUiEvent(
      'chat.send_success',
      result?.offline ? 'offline' : 'ok',
      'Chat message answered.',
      {
        chatId: state.activeChatId,
        provider: result?.provider || state.llmProvider,
        model: result?.model || state.providerModel,
      },
    );
  } catch (error) {
    const message = normalizeError(error);
    appendChatMessage('argentum', `Live chat failed: ${message}`, {
      status: 'error',
      error: message,
    });
    recordUiEvent('chat.send_failed', 'error', message, {
      chatId: state.activeChatId,
    });
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
  if (target.id === 'provider-api-key' || target.id === 'settings-provider-api-key') {
    state.providerApiKey = target.value;
  }
  if (target.id === 'custom-provider-name') state.customProviderName = target.value;
  if (target.id === 'custom-api-key-env') state.customApiKeyEnv = target.value;
  if (target.id === 'webchat-token') state.webchatToken = target.value;
  if (target.id === 'telegram-token') state.telegramToken = target.value;
  if (target.id === 'telegram-allowlist') state.telegramAllowlist = target.value;
  if (target.id === 'whatsapp-phone-id') state.whatsappPhoneId = target.value;
  if (target.id === 'chat-draft') {
    state.draftMessage = target.value;
    syncComposerSendState();
    return;
  }
  if (target.id === 'profile-user-name') state.userName = target.value;
  if (target.id === 'onboarding-user-name') state.userName = target.value;
  if (target.id === 'profile-agent-name') state.agentName = target.value || 'Argentum';
  if (target.id === 'onboarding-agent-name') state.agentName = target.value || 'Argentum';
  if (target.id === 'profile-purpose') state.systemPrompt = target.value;
  if (target.id === 'onboarding-system-prompt') state.systemPrompt = target.value;
}

async function handleChange(event) {
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

  if (target.id === 'onboarding-provider-select') {
    updateProviderFieldsFromPreset(target.value);
    render();
    return;
  }

  if (target.id === 'provider-model' || target.id === 'settings-provider-model') {
    if (target.id === 'provider-model') {
      selectModel(target.value);
    } else {
      state.providerModel = target.value;
      state.apiTest = {
        status: 'idle',
        message: 'Model changed. Test the provider before using live chat.',
      };
    }
    render();
    return;
  }

  if (target.id === 'provider-auth-method' || target.id === 'settings-provider-auth-method') {
    if (target.id === 'provider-auth-method') {
      selectAuthMethod(target.value);
    } else {
      state.providerAuthMethod = target.value;
      state.apiTest = {
        status: target.value === 'browser-account' ? 'warning' : 'idle',
        message:
          target.value === 'browser-account'
            ? 'OpenAI/Codex authorization selected. Start and complete authorization before testing live chat.'
            : 'Authorization method changed. Test the provider before using live chat.',
      };
    }
    render();
    return;
  }

  if (target.id === 'thinking-level') {
    state.thinkingLevel = target.value;
    notify('info', 'Thinking level changed', `Chat thinking level set to ${target.value}.`);
    if (state.setupComplete) await persistRuntimeSettings('thinking-level', { notify: false });
    render();
    return;
  }

  if (target.id === 'settings-show-thinking-chat') {
    state.showThinkingInChat = target.checked;
    notify(
      'info',
      'Reasoning display changed',
      state.showThinkingInChat
        ? 'Thinking and reasoning blocks will appear as collapsible chat context.'
        : 'Thinking and reasoning blocks will stay hidden in chat.',
    );
    if (state.setupComplete) await persistRuntimeSettings('reasoning-output', { notify: false });
    render();
    return;
  }

  if (target.id === 'settings-show-thinking-telegram') {
    state.showThinkingInTelegram = target.checked;
    notify(
      'info',
      'Telegram reasoning changed',
      state.showThinkingInTelegram
        ? 'Telegram may include separated reasoning blocks.'
        : 'Telegram will omit separated reasoning blocks.',
    );
    if (state.setupComplete) await persistRuntimeSettings('reasoning-output', { notify: false });
    render();
    return;
  }

  if (target.id === 'settings-ui-font') {
    setUiPreference('uiFontFamily', target.value);
    render();
    return;
  }

  if (target.id === 'settings-code-font') {
    setUiPreference('codeFontFamily', target.value);
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
    await applyRuntimeMode('runtime-mode');
    render();
  }
}

async function handleKeyDown(event) {
  const target = event.target;
  if (!(target instanceof HTMLTextAreaElement)) return;
  if (target.id !== 'chat-draft') return;
  if (
    event.key !== 'Enter' ||
    event.shiftKey ||
    event.ctrlKey ||
    event.altKey ||
    event.metaKey ||
    event.isComposing
  )
    return;

  event.preventDefault();
  state.draftMessage = target.value;
  syncComposerSendState();
  await sendChatDraft();
}

async function handleClick(event, activationElement = null) {
  const element = activationElement || eventTargetElement(event);
  if (!element) return;
  if (window.location.search.includes('debugEvents=1')) {
    console.debug(
      '[Argentum handleClick]',
      JSON.stringify({
        target: element.tagName,
        id: element.id,
        external: Boolean(element.closest('[data-open-external]')),
        notifications: Boolean(element.closest('#notifications-button')),
        help: Boolean(element.closest('#help-button')),
        viewMode: element.closest('button[data-view-mode]')?.getAttribute('data-view-mode') || '',
        workspace: Boolean(element.closest('#workspace-button')),
        nav: element.closest('[data-section]')?.getAttribute('data-section') || '',
        onboardingStep:
          element.closest('[data-onboarding-step]')?.getAttribute('data-onboarding-step') || '',
        experience:
          element.closest('[data-experience-level]')?.getAttribute('data-experience-level') || '',
        next: Boolean(element.closest('#next-button')),
      }),
    );
  }

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

  if (element.closest('[data-close-notifications]')) {
    toggleNotificationsMenu(false);
    render();
    return;
  }

  if (element.closest('#help-button')) {
    toggleHelp();
    render();
    return;
  }

  if (element.closest('[data-close-help]')) {
    toggleHelp(false);
    render();
    return;
  }

  if (element.closest('#audit-button')) {
    toggleQuickSecurityMenu();
    render();
    return;
  }

  if (element.closest('#settings-button')) {
    toggleQuickSettingsMenu();
    render();
    return;
  }

  const closeQuickMenu = element.closest('[data-close-quick-menu]');
  if (closeQuickMenu) {
    if (closeQuickMenu.dataset.closeQuickMenu === 'security') toggleQuickSecurityMenu(false);
    if (closeQuickMenu.dataset.closeQuickMenu === 'settings') toggleQuickSettingsMenu(false);
    render();
    return;
  }

  const viewModeButton = element.closest('button[data-view-mode]');
  if (viewModeButton) {
    setViewMode(viewModeButton.dataset.viewMode);
    render();
    return;
  }

  if (element.closest('#workspace-button')) {
    toggleWorkspaceMenu();
    render();
    return;
  }

  if (element.closest('[data-close-workspace-menu]')) {
    toggleWorkspaceMenu(false);
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
    const result = goToStep(Number(stepButton.dataset.onboardingStep));
    if (!result.ok) notify('error', 'Setup needs a fix', result.message);
    render();
    return;
  }

  const experienceButton = element.closest('[data-experience-level]');
  if (experienceButton) {
    if (window.location.search.includes('debugEvents=1')) {
      console.debug(
        '[Argentum action] selectExperienceLevel',
        experienceButton.dataset.experienceLevel,
      );
    }
    const result = selectExperienceLevel(experienceButton.dataset.experienceLevel);
    if (!result.ok) notify('error', 'Setup needs a fix', result.message);
    render();
    return;
  }

  const runtimeButton = element.closest('[data-runtime-mode]');
  if (runtimeButton) {
    state.runtimeMode = runtimeButton.dataset.runtimeMode;
    if (runtimeButton.closest('.runtime-mode-tabs')) {
      await applyRuntimeMode('runtime-mode');
    }
    render();
    return;
  }

  const providerCatalogButton = element.closest('[data-provider-catalog-tab]');
  if (providerCatalogButton) {
    state.providerCatalogTab = providerCatalogButton.dataset.providerCatalogTab;
    clearOnboardingError();
    render();
    return;
  }

  const providerButton = element.closest('[data-provider-id]');
  if (providerButton) {
    const result = updateProviderFieldsFromPreset(providerButton.dataset.providerId);
    if (!result.ok) notify('error', 'Setup needs a fix', result.message);
    render();
    return;
  }

  const providerAuthButton = element.closest('[data-provider-auth-method]');
  if (providerAuthButton) {
    const result = selectAuthMethod(providerAuthButton.dataset.providerAuthMethod);
    if (!result.ok) notify('error', 'Setup needs a fix', result.message);
    render();
    return;
  }

  const providerStageButton = element.closest('[data-provider-setup-stage]');
  if (providerStageButton) {
    const result = setProviderSetupStage(
      providerStageButton.dataset.providerSetupStage || 'provider',
    );
    if (!result.ok) notify('error', 'Setup needs a fix', result.message);
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
    const stageResult = setProviderSetupStage('model');
    if (!stageResult.ok) {
      notify('error', 'Setup needs a fix', stageResult.message);
      render();
      return;
    }
    const result = goToStep(5);
    if (!result.ok) notify('error', 'Setup needs a fix', result.message);
    render();
    return;
  }

  if (element.closest('#attach-file')) {
    await chooseChatAttachment();
    render();
    return;
  }

  const removeAttachment = element.closest('[data-remove-attachment]');
  if (removeAttachment) {
    state.chatAttachments = state.chatAttachments.filter(
      (file) => file.id !== removeAttachment.dataset.removeAttachment,
    );
    render();
    return;
  }

  if (element.closest('[data-stop-generation]')) {
    state.chatAbortRequested = true;
    notify('info', 'Generation stopping', 'Argentum will stop updating the current answer.');
    render();
    return;
  }

  if (element.closest('#voice-input')) {
    startVoiceInput();
    render();
    return;
  }

  if (element.closest('#start-codex-oauth')) {
    selectAuthMethod('browser-account');
    await startCodexOAuth();
    render();
    return;
  }

  if (element.closest('#complete-codex-oauth')) {
    selectAuthMethod('browser-account');
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

  const chatFilterButton = element.closest('[data-chat-filter]');
  if (chatFilterButton) {
    setChatFilter(chatFilterButton.dataset.chatFilter);
    render();
    return;
  }

  const settingsSectionButton = element.closest('[data-settings-section]');
  if (settingsSectionButton) {
    setSettingsSection(settingsSectionButton.dataset.settingsSection);
    render();
    return;
  }

  const conversationMenuButton = element.closest('[data-conversation-menu]');
  if (conversationMenuButton) {
    toggleConversationMenu(conversationMenuButton.dataset.conversationMenu);
    render();
    return;
  }

  if (element.closest('[data-conversation-settings]')) {
    const active = state.chatSessions.find((chat) => chat.id === state.activeChatId);
    if (active) toggleConversationMenu(active.id);
    render();
    return;
  }

  const pinChatButton = element.closest('[data-pin-chat]');
  if (pinChatButton) {
    toggleChatPinned(pinChatButton.dataset.pinChat);
    render();
    return;
  }

  const clearChatButton = element.closest('[data-clear-chat]');
  if (clearChatButton) {
    clearChatSession(clearChatButton.dataset.clearChat);
    notify('info', 'Conversation cleared', 'Messages were removed from that local chat.');
    render();
    return;
  }

  const renameChatButton = element.closest('[data-rename-chat]');
  if (renameChatButton) {
    const chat = state.chatSessions.find((item) => item.id === renameChatButton.dataset.renameChat);
    const nextTitle = window.prompt('Rename conversation', chat?.title || 'New chat');
    if (nextTitle) renameChatSession(renameChatButton.dataset.renameChat, nextTitle);
    render();
    return;
  }

  const deleteChatButton = element.closest('[data-delete-chat]');
  if (deleteChatButton) {
    requestDeleteChatSession(deleteChatButton.dataset.deleteChat);
    render();
    return;
  }

  const confirmDeleteChatButton = element.closest('[data-confirm-delete-chat]');
  if (confirmDeleteChatButton) {
    confirmDeleteChatSession(confirmDeleteChatButton.dataset.confirmDeleteChat);
    notify('info', 'Chat deleted', 'The selected chat was removed from local history.');
    render();
    return;
  }

  if (element.closest('[data-cancel-delete-chat]')) {
    cancelDeleteChatSession();
    render();
    return;
  }

  if (element.closest('#new-chat')) {
    createChatSession();
    render();
    return;
  }

  if (element.closest('#back-button')) {
    previousStep();
    render();
    return;
  }

  if (element.closest('#next-button')) {
    if (state.onboardingStep === onboardingSteps.length) {
      await finishOnboarding();
    } else {
      advanceOnboarding();
    }
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

  const chatPromptButton = element.closest('[data-chat-prompt]');
  if (chatPromptButton) {
    state.draftMessage = chatPromptButton.dataset.chatPrompt || chatPromptButton.textContent.trim();
    render();
    return;
  }

  const copyMessageButton = element.closest('[data-copy-message]');
  if (copyMessageButton) {
    const block = state.chatBlocks.find(
      (item) => item.id === copyMessageButton.dataset.copyMessage,
    );
    if (block?.body) {
      try {
        await navigator.clipboard?.writeText(block.body);
        notify('success', 'Message copied', 'The message text is on your clipboard.');
      } catch (_error) {
        notify('info', 'Message ready', block.body.slice(0, 120));
      }
    }
    render();
    return;
  }

  const regenerateButton = element.closest('[data-regenerate-message], [data-retry-message]');
  if (regenerateButton) {
    await regenerateAssistantResponse(
      regenerateButton.dataset.regenerateMessage || regenerateButton.dataset.retryMessage,
    );
    return;
  }

  if (element.closest('[data-refresh-state]')) {
    await refreshDesktopState({ announce: true });
    render();
    return;
  }

  if (element.closest('[data-compact-context]')) {
    const compacted = compactActiveChatSession();
    notify(
      compacted ? 'success' : 'info',
      compacted ? 'Context compacted' : 'Context already compact',
      compacted
        ? 'Older messages were summarized into a compact context block.'
        : 'This chat is short enough that no compaction was needed.',
    );
    state.chatScrollIntent = 'bottom';
    render();
    return;
  }

  const approvalButton = element.closest('[data-approval]');
  if (approvalButton) {
    state.actionStatus = `Review opened for ${approvalButton.dataset.approval}.`;
    notify(
      'info',
      'Approval review',
      'Detailed approval editing will open here when the broker is wired.',
    );
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
      notify(
        'info',
        'Profile panel',
        'Use the profile fields on the right side of Chat to set your name and the agent name.',
      );
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

  if (element.closest('#save-settings')) {
    await saveSettingsFromInputs();
    render();
    return;
  }

  if (element.closest('#send-chat')) {
    await sendChatDraft();
  }
}

addActivationListeners(document);
addActivationListeners(document.body);
addActivationListeners(viewRoot);
addActivationListeners(overlayRoot);
document.addEventListener('input', handleInput);
document.addEventListener('change', handleChange);
document.addEventListener('keydown', handleKeyDown);
document.addEventListener(
  'toggle',
  (event) => {
    const target = event.target;
    if (!(target instanceof HTMLDetailsElement)) return;
    const disclosure = target.closest('details.setup-disclosure[data-disclosure-id]');
    if (!(disclosure instanceof HTMLDetailsElement)) return;
    setOnboardingDisclosure(disclosure.dataset.disclosureId, disclosure.open);
  },
  true,
);
window.addEventListener('argentum:state-change', render);

hydrateStaticIcons(document);
hydrateUiPreferences();
const chatHistoryRestored = hydrateChatHistory();
hydrateOnboardingProgress();
scheduleVisibleNotifications();
hydrateDesktopDefaults()
  .then(() => refreshDesktopState())
  .then(() => {
    if (state.desktopState?.configExists) {
      state.setupComplete = true;
      state.onboardingOpen = false;
      clearOnboardingProgress();
      state.activeSection = 'chat';
      state.notifications = [];
      if (!chatHistoryRestored) resetIntroChat();
    } else {
      state.onboardingOpen = true;
    }
  })
  .finally(() => render());
