import {
  commandCatalog,
  llamaDownloadPresets,
  modelMetadata,
  onboardingSteps,
  providerPresets,
  sections,
} from './modules/constants.js';
import { hydrateStaticIcons } from './modules/icons.js';
import { modules } from './modules/sections.js';
import { setLocale, textDirection } from './i18n/index.js';
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
  setLlamaServerConfig,
  setUiPreference,
  setViewMode,
  state,
  syncActiveChatSession,
  requestDeleteChatSession,
  toggleChatPinned,
  toggleChatPanel,
  toggleConversationMenu,
  toggleHelp,
  toggleNotificationsMenu,
  toggleNotificationsMuted,
  toggleQuickSecurityMenu,
  toggleQuickSettingsMenu,
  toggleWorkspaceMenu,
  updateChatMessage,
  persistUiPreferences,
  setSkillsCatalog,
  setInstalledSkills,
  setSkillsTab,
  setSkillsSearch,
  setSkillsCategory,
} from './modules/state.js';
import {
  ANTHROPIC_SKILLS,
  CODEX_CURATED_SKILLS,
  CODEX_SYSTEM_SKILLS,
} from './modules/skills-catalog.js';
import {
  chooseWorkspaceFolder,
  completeCodexOAuth,
  buildChatRequestPayload,
  handleMigrationImport,
  openExternalUrl,
  persistRuntimeSettings,
  refreshDesktopState,
  refreshSystemDashboardState,
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
  currentProvider,
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
let dashboardRefreshTimer = null;

// Focus trap for modal dialogs - keeps keyboard focus within open panels
const FOCUSABLE_SELECTOR =
  'button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function getFocusableElements(container) {
  return [...container.querySelectorAll(FOCUSABLE_SELECTOR)].filter(
    (el) => el.offsetParent !== null && getComputedStyle(el).visibility !== 'hidden',
  );
}

function trapFocus(container) {
  const focusable = getFocusableElements(container);
  if (focusable.length === 0) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];

  function handleTab(e) {
    if (e.key !== 'Tab') return;
    if (e.shiftKey) {
      if (document.activeElement === first) {
        e.preventDefault();
        last.focus();
      }
    } else {
      if (document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  container.addEventListener('keydown', handleTab);
  first.focus();
  return () => container.removeEventListener('keydown', handleTab);
}

function applyUiPreferences() {
  document.documentElement.style.setProperty('--font-ui', state.uiFontFamily);
  document.documentElement.style.setProperty('--font-mono', state.codeFontFamily);
  if (state.accentColor) {
    document.documentElement.style.setProperty('--accent', state.accentColor);
    // Derive soft variant from accent
    const hex = state.accentColor.replace('#', '');
    const r = parseInt(hex.substring(0, 2), 16);
    const g = parseInt(hex.substring(2, 4), 16);
    const b = parseInt(hex.substring(4, 6), 16);
    document.documentElement.style.setProperty('--accent-soft', `rgba(${r}, ${g}, ${b}, 0.15)`);
  } else {
    document.documentElement.style.removeProperty('--accent');
    document.documentElement.style.removeProperty('--accent-soft');
  }
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
  const onboardingScroll = captureOnboardingStepScroll();
  applyUiPreferences();
  const section = activeSection();
  const module = modules[section.id] || modules.chat;

  workspacePath.textContent = state.workspacePath;
  title.textContent = section.title;
  eyebrow.textContent = section.eyebrow;
  nav.innerHTML = renderNavigation();
  // Add update indicator class if update is available
  const updateNavButton = nav.querySelector('[data-section="update"]');
  if (updateNavButton) {
    updateNavButton.classList.toggle('has-update', Boolean(state.updateAvailable));
  }
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
  syncSystemDashboardPolling();
  syncComposerSendState();
  scrollTerminalPanels();
  restoreChatTranscriptScroll(chatScroll);
  restoreOnboardingStepScroll(onboardingScroll);
}

function renderFloatingPanels(section) {
  return `${renderHelpPanel(section)}${renderLocalServerGuidePanel()}${renderNotificationMenu()}${renderQuickSecurityPanel()}${renderQuickSettingsPanel()}${renderWorkspacePanel()}`;
}

function captureChatTranscriptScroll() {
  const panel = document.querySelector('.chat-transcript');
  if (!(panel instanceof HTMLElement)) return null;
  return {
    top: panel.scrollTop,
    height: panel.scrollHeight,
    pinned: isChatTranscriptPinned(panel),
  };
}

function isChatTranscriptPinned(panel) {
  if (!(panel instanceof HTMLElement)) return true;
  return panel.scrollTop + panel.clientHeight >= panel.scrollHeight - 96;
}

function scrollChatTranscriptToBottom(panel) {
  if (!(panel instanceof HTMLElement)) return;
  panel.scrollTop = panel.scrollHeight;
  const schedule = window.requestAnimationFrame || ((callback) => window.setTimeout(callback, 0));
  schedule(() => {
    panel.scrollTop = panel.scrollHeight;
    window.setTimeout(() => {
      panel.scrollTop = panel.scrollHeight;
    }, 40);
  });
}

function requestChatScrollBottom(force = false) {
  if (force || state.chatAutoFollow !== false) {
    state.chatScrollIntent = 'bottom';
    state.chatAutoFollow = true;
    state.chatHasNewTransmission = false;
    return;
  }
  state.chatHasNewTransmission = true;
}

function restoreChatTranscriptScroll(previous) {
  const schedule = window.requestAnimationFrame || ((callback) => window.setTimeout(callback, 0));
  schedule(() => {
    const panel = document.querySelector('.chat-transcript');
    if (!(panel instanceof HTMLElement)) return;

    if (state.chatScrollIntent === 'bottom') {
      scrollChatTranscriptToBottom(panel);
      state.chatScrollIntent = '';
      state.chatAutoFollow = true;
      state.chatHasNewTransmission = false;
      return;
    }

    if (!previous) return;
    if (previous.pinned) {
      scrollChatTranscriptToBottom(panel);
      state.chatAutoFollow = true;
      return;
    }

    panel.scrollTop = Math.max(0, previous.top);
    state.chatAutoFollow = isChatTranscriptPinned(panel);
  });
}

function captureOnboardingStepScroll() {
  const panel = document.querySelector('.onboarding-step-panel');
  if (!(panel instanceof HTMLElement)) return null;
  const activeStep = document.querySelector('.onboarding-step-list .step-chip.active');
  const step = Number(activeStep?.getAttribute('data-onboarding-step') || state.onboardingStep);
  return {
    step,
    top: panel.scrollTop,
    left: panel.scrollLeft,
  };
}

function restoreOnboardingStepScroll(previous) {
  if (!previous || previous.step !== state.onboardingStep) return;
  const schedule = window.requestAnimationFrame || ((callback) => window.setTimeout(callback, 0));
  schedule(() => {
    const panel = document.querySelector('.onboarding-step-panel');
    if (!(panel instanceof HTMLElement)) return;
    panel.scrollTop = previous.top;
    panel.scrollLeft = previous.left;
  });
}

function renderHelpPanel(section) {
  if (!state.helpOpen) return '';

  // Maps section IDs to their GitHub wiki / docs URLs
  const docLinks = {
    'chat':
      'https://github.com/AG064/argentum/blob/development/docs/USER_GUIDE.md#2-agent-creation-and-management',
    'gateway':
      'https://github.com/AG064/argentum/blob/development/docs/USER_GUIDE.md#6-deployment-options',
    'local-server':
      'https://github.com/AG064/argentum/blob/development/docs/USER_GUIDE.md#local-development',
    'security':
      'https://github.com/AG064/argentum/blob/development/docs/USER_GUIDE.md#5-security-best-practices',
    'pc-stats':
      'https://github.com/AG064/argentum/blob/development/docs/USER_GUIDE.md#7-monitoring-and-logging',
    'settings':
      'https://github.com/AG064/argentum/blob/development/docs/USER_GUIDE.md#agent-configuration',
    'diagnostics':
      'https://github.com/AG064/argentum/blob/development/docs/QUICK_START.md#troubleshooting-common-issues',
    'logs': 'https://github.com/AG064/argentum/blob/development/docs/USER_GUIDE.md#viewing-logs',
    'update': 'https://github.com/AG064/argentum/releases',
    'onboarding': 'https://github.com/AG064/argentum/blob/development/docs/QUICK_START.md',
  };

  const tips = {
    chat: [
      [
        'Chat view',
        'CHAT is the active layout for now. Conversations stay on the left, chat stays centered, and the inspector stays on the right.',
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
  const docUrl = docLinks[section.id];
  const sectionTitleId = section.id === 'pc-stats' ? 'system-dashboard' : section.id;

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
      ${
        docUrl
          ? `
      <div class="help-doc-link">
        <a href="${escapeAttribute(docUrl)}" target="_blank" rel="noopener noreferrer" data-open-external="${escapeAttribute(docUrl)}">
          Open full docs for ${escapeHtml(section.title)} <span data-icon="externalLink"></span>
        </a>
        <a href="https://github.com/AG064/argentum/blob/development/docs/FAQ.md" target="_blank" rel="noopener noreferrer" data-open-external="https://github.com/AG064/argentum/blob/development/docs/FAQ.md">
          Open FAQ <span data-icon="externalLink"></span>
        </a>
      </div>
      `
          : ''
      }
      <div class="help-shortcuts">
        <strong>Keyboard shortcuts</strong>
        <dl>
          <dt><kbd>?</kbd></dt><dd>Toggle this help panel</dd>
          <dt><kbd>Esc</kbd></dt><dd>Close open panels</dd>
          <dt><kbd>Ctrl+1</kbd> - <kbd>Ctrl+8</kbd></dt><dd>Navigate sections</dd>
          <dt><kbd>Ctrl+,</kbd></dt><dd>Open Settings</dd>
          <dt><kbd>Enter</kbd> (in chat)</dt><dd>Send message</dd>
          <dt><kbd>Shift+Enter</kbd></dt><dd>New line in chat</dd>
          <dt><kbd>Tab</kbd></dt><dd>Move focus forward</dd>
          <dt><kbd>Shift+Tab</kbd></dt><dd>Move focus back</dd>
        </dl>
      </div>
    </aside>
  `;
}

function renderLocalServerGuidePanel() {
  if (!state.localServerGuideOpen) return '';

  return `
    <aside class="help-panel floating-panel local-server-guide" role="dialog" aria-label="Local server guide">
      <div class="split-header">
        <div>
          <span class="pill ok">Local Server</span>
          <h3>Finish local model setup</h3>
        </div>
        <button class="icon-button compact" data-close-local-server-guide="true" aria-label="Close local server guide"><span data-icon="x"></span></button>
      </div>
      <div class="help-callout-list">
        <article>
          <strong>1. Open Local Server</strong>
          <p>Choose the llama.cpp tab and confirm the model source. Hugging Face presets download through llama.cpp automatically.</p>
        </article>
        <article>
          <strong>2. Start llama.cpp</strong>
          <p>Press Start Server. Argentum shows download/load progress while the server prepares the model.</p>
        </article>
        <article>
          <strong>3. Test in Chat</strong>
          <p>Once the endpoint is ready, use Test Provider or send a small message through the local model.</p>
        </article>
      </div>
      <div class="button-row">
        <button class="button primary" data-section="local-server" data-close-local-server-guide="true">Open Local Server</button>
        <button class="button" data-close-local-server-guide="true">Later</button>
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
    setUiPreference('workspacePath', state.workspacePath);
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
    if (state.llmProvider === 'llama-cpp' || state.selectedContextAccess.includes('local-server')) {
      state.localServerGuideOpen = true;
      state.activeSection = 'local-server';
      notify(
        'info',
        'Local model setup',
        'Open Local Server to download or start the selected llama.cpp model.',
      );
    }
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
      'Preview mode: the installed Tauri app is required to execute desktop runtime actions.',
      'warning',
    );
    notify('warning', 'Desktop bridge unavailable', state.actionStatus);
    state.runningAction = '';
    render();
    return;
  }

  try {
    const result = await invoke('run_desktop_action', {
      request: {
        actionId,
        workspacePath: state.workspacePath,
        llamaServer: actionId.startsWith('llama-server-') ? state.llamaServerConfig : undefined,
      },
    });
    const status = result?.status || 'ok';
    const output = result?.output || result?.message || 'Action completed without output.';
    const actualCommand = result?.command || command;
    state.actionStatus = result?.message || `${action?.title || actionId} completed.`;
    addTerminalEntry(actualCommand, output, status === 'error' ? 'error' : 'success');
    notify(
      status === 'stopped' || status === 'starting' ? 'info' : 'success',
      action?.title || 'Desktop action',
      state.actionStatus,
    );
    await refreshDesktopState();
    if (actionId === 'llama-server-start' && status === 'starting') {
      startLlamaServerProgressPolling();
    }
  } catch (error) {
    const message = normalizeError(error);
    addTerminalEntry(command, message, 'error');
    notify('error', 'Action failed', message);
  } finally {
    state.runningAction = '';
  }
  render();
}

function progressFromLlamaLog(logPreview = '') {
  const log = String(logPreview || '');
  const percentMatch = [...log.matchAll(/(\d{1,3})(?:\.\d+)?\s*%/g)]
    .map((match) => Number.parseInt(match[1], 10))
    .filter((value) => Number.isFinite(value) && value >= 0 && value <= 100)
    .pop();
  if (Number.isFinite(percentMatch)) {
    return { percent: percentMatch, phase: 'Downloading model' };
  }
  if (/server is listening|model loaded/i.test(log)) return { percent: 100, phase: 'Ready' };
  if (/warming up/i.test(log)) return { percent: 92, phase: 'Warming model' };
  if (/initializing slots|chat template/i.test(log))
    return { percent: 84, phase: 'Preparing slots' };
  if (/loading model/i.test(log)) return { percent: 70, phase: 'Loading model' };
  if (/download|huggingface|cache|snapshot|resolve/i.test(log)) {
    return { percent: 35, phase: 'Downloading or resolving model' };
  }
  return { percent: 12, phase: 'Starting llama.cpp' };
}

async function startLlamaServerProgressPolling() {
  const startedAt = Date.now();
  state.llamaServerProgress = {
    active: true,
    percent: 8,
    phase: 'Starting llama.cpp',
    detail: 'Waiting for model download or load logs.',
    startedAt,
  };
  render();

  const deadline = startedAt + 2 * 60 * 1000;
  while (Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 1500));
    await refreshDesktopState();
    const pid = state.desktopState?.llamaServerPid;
    const logPreview = state.desktopState?.llamaServerLogPreview || '';
    const progress = progressFromLlamaLog(logPreview);
    state.llamaServerProgress = {
      active: Boolean(pid) && progress.percent < 100,
      percent: progress.percent,
      phase: progress.phase,
      detail:
        progress.percent >= 100
          ? 'Local server is ready.'
          : 'Downloads are handled by llama.cpp. Exact percent is shown when llama.cpp emits it; otherwise this follows startup phases.',
      startedAt,
    };
    render();
    if (!pid || progress.percent >= 100) return;
  }

  state.llamaServerProgress = {
    active: false,
    percent: state.llamaServerProgress?.percent || 0,
    phase: 'Still starting',
    detail: 'The server is still running. Open Local Server logs for current download/load output.',
    startedAt,
  };
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
      ? ''
      : 'Ready to send.';
  const title = state.chatStreaming
    ? 'Generation in progress. Stop it before sending another message.'
    : disabled
      ? 'Enter a message or attach a file.'
      : 'Ready to send.';

  if (sendButton instanceof HTMLButtonElement) {
    sendButton.disabled = disabled;
    sendButton.setAttribute('aria-disabled', String(disabled));
    sendButton.title = title;
  }
  if (status instanceof HTMLElement) {
    status.textContent = message;
  }
}

function updateProviderKeyStatus() {
  const status = document.querySelector('#provider-key-status');
  if (!(status instanceof HTMLElement)) return;
  const provider = currentProvider(providerPresets, state);
  const keyPresent = state.providerApiKey.trim().length > 0;
  status.classList.toggle('hidden', Boolean(provider.requiresKey && keyPresent));
  status.textContent = provider.requiresKey ? 'Key required' : 'Key optional';
}

async function checkForUpdates() {
  state.updateDownloading = true;
  state.updateError = '';
  render();

  try {
    const invoke = window.__TAURI__?.core?.invoke;
    if (invoke) {
      const result = await invoke('check_for_updates');
      state.updateChecked = true;
      if (result?.updateAvailable) {
        state.updateAvailable = true;
        state.updateVersion = result.version || '';
        notify('info', 'Update available', `Argentum ${state.updateVersion} is available.`);
      } else {
        state.updateAvailable = false;
        state.updateVersion = '';
        notify('info', 'Up to date', 'You are running the latest Argentum version.');
      }
    } else {
      // Web preview mode cannot make an authoritative desktop update check.
      state.updateAvailable = false;
      state.updateChecked = false;
      state.updateVersion = '';
      notify('info', 'Desktop app required', 'Update check requires the desktop app.');
    }
  } catch (error) {
    state.updateChecked = false;
    state.updateError = normalizeError(error);
  } finally {
    state.updateDownloading = false;
    render();
  }
}

async function downloadUpdate() {
  if (!state.updateVersion) return;
  state.updateDownloading = true;
  state.updateError = '';
  render();

  try {
    const invoke = window.__TAURI__?.core?.invoke;
    if (invoke) {
      const releaseUrl = await invoke('download_update');
      await openExternalUrl(releaseUrl);
      notify(
        'success',
        'Releases page opened',
        `Visit ${state.updateVersion} on GitHub to download the installer for your platform.`,
      );
    } else {
      notify('info', 'Desktop app required', 'Opening the releases page requires the desktop app.');
    }
  } catch (error) {
    state.updateError = normalizeError(error);
    notify('error', 'Could not open releases', state.updateError);
  } finally {
    state.updateDownloading = false;
    render();
  }
}

async function searchHuggingFaceModels() {
  const query = String(state.huggingFaceSearch?.query || '').trim();
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    notify('warning', 'Desktop app required', 'Hugging Face search uses the desktop bridge.');
    return;
  }
  state.huggingFaceSearch = { ...state.huggingFaceSearch, status: 'loading', error: '' };
  render();
  try {
    const results = await invoke('search_huggingface_models', { query });
    state.huggingFaceSearch = {
      ...state.huggingFaceSearch,
      status: 'complete',
      error: '',
      results: Array.isArray(results) ? results : [],
    };
    if (!state.huggingFaceSearch.results.length) {
      notify('info', 'No GGUF models found', 'Try a broader Hugging Face search.');
    }
  } catch (error) {
    state.huggingFaceSearch = {
      ...state.huggingFaceSearch,
      status: 'error',
      error: normalizeError(error),
      results: [],
    };
  }
  render();
}

async function scanLocalModels() {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    notify('warning', 'Desktop app required', 'Local model scan uses the desktop bridge.');
    return;
  }
  state.localModelScan = { ...state.localModelScan, status: 'loading', error: '' };
  render();
  try {
    const results = await invoke('scan_local_models', { workspacePath: state.workspacePath });
    state.localModelScan = {
      status: 'complete',
      error: '',
      results: Array.isArray(results) ? results : [],
    };
  } catch (error) {
    state.localModelScan = {
      status: 'error',
      error: normalizeError(error),
      results: [],
    };
  }
  render();
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

  // Apply focus trap to open modal dialogs
  for (const panel of overlayRoot?.querySelectorAll('[role="dialog"]') || []) {
    if (!(panel instanceof HTMLElement)) continue;
    if (panel.dataset.argentumFocusTrapped === 'true') continue;
    panel.dataset.argentumFocusTrapped = 'true';
    trapFocus(panel);
  }

  const dashboardFrame = document.querySelector('[data-system-dashboard-frame]');
  if (
    dashboardFrame instanceof HTMLIFrameElement &&
    dashboardFrame.dataset.argentumFrameWired !== 'true'
  ) {
    dashboardFrame.dataset.argentumFrameWired = 'true';
    dashboardFrame.addEventListener('load', syncSystemDashboardFrame);
  }

  const chatTranscript = document.querySelector('.chat-transcript');
  if (
    chatTranscript instanceof HTMLElement &&
    chatTranscript.dataset.argentumScrollWired !== 'true'
  ) {
    chatTranscript.dataset.argentumScrollWired = 'true';
    chatTranscript.addEventListener(
      'scroll',
      () => {
        const pinned = isChatTranscriptPinned(chatTranscript);
        state.chatAutoFollow = pinned;
        if (pinned && state.chatHasNewTransmission) {
          state.chatHasNewTransmission = false;
          render();
        }
      },
      { passive: true },
    );
  }
}

function syncSystemDashboardFrame() {
  const dashboardFrame = document.querySelector('[data-system-dashboard-frame]');
  if (!(dashboardFrame instanceof HTMLIFrameElement) || !dashboardFrame.contentWindow) return;
  const dashboardEnabled = state.selectedContextAccess.includes('system-dashboard');
  dashboardFrame.contentWindow.postMessage(
    {
      type: 'argentum-system-stats',
      stats: dashboardEnabled ? state.desktopState?.systemStats || null : null,
    },
    '*',
  );
}

function syncSystemDashboardPolling() {
  const dashboardEnabled = state.selectedContextAccess.includes('system-dashboard');
  const dashboardVisible =
    dashboardEnabled &&
    state.activeSection === 'pc-stats' &&
    document.querySelector('[data-system-dashboard-frame]') instanceof HTMLIFrameElement;

  if (!dashboardVisible) {
    if (dashboardRefreshTimer) {
      window.clearInterval(dashboardRefreshTimer);
      dashboardRefreshTimer = null;
    }
    return;
  }

  if (dashboardRefreshTimer) return;
  const refreshDashboardFrame = async () => {
    if (state.activeSection !== 'pc-stats') {
      syncSystemDashboardPolling();
      return;
    }
    try {
      await refreshSystemDashboardState({ silentErrors: true });
      syncSystemDashboardFrame();
    } catch (error) {
      console.warn('[Argentum dashboard refresh failed]', normalizeError(error));
    }
  };
  dashboardRefreshTimer = window.setInterval(refreshDashboardFrame, 10_000);
  void refreshDashboardFrame();
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
      id: `${Date.now()}-${(() => {
        const rand = new Uint8Array(8);
        crypto.getRandomValues(rand);
        return [...rand].map((b) => b.toString(16).padStart(2, '0')).join('');
      })()}`,
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

async function chooseLlamaModelFile() {
  const selected = await openFile(state.workspacePath, {
    filters: [{ name: 'GGUF models', extensions: ['gguf'] }],
  });
  if (!selected) {
    notify('info', 'Model path unchanged', 'No GGUF model was selected.');
    return;
  }

  const path = Array.isArray(selected) ? selected[0] : selected;
  setLlamaServerConfig('modelSource', 'file');
  setLlamaServerConfig('modelPath', path);
  notify(
    'success',
    'Local model selected',
    'The llama.cpp server will use the selected GGUF model path.',
  );
  await promptLlamaServerRestart('Local model path changed');
}

async function chooseUserAvatarFile() {
  const selected = await openFile(state.workspacePath, {
    filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }],
  });
  if (!selected) {
    notify('info', 'Avatar unchanged', 'No image was selected.');
    return;
  }

  const path = Array.isArray(selected) ? selected[0] : selected;
  setUiPreference('userAvatarPath', path);
  notify('success', 'Avatar updated', 'Your chat avatar now uses the selected image.');
}

function clearUserAvatarFile() {
  setUiPreference('userAvatarPath', '');
  notify('info', 'Avatar cleared', 'Your chat avatar will use initials again.');
}

async function promptLlamaServerRestart(reason) {
  if (!state.setupComplete || state.llmProvider !== 'llama-cpp') return;
  await refreshDesktopState({ silentErrors: true });
  if (!state.desktopState?.llamaServerPid) return;

  const shouldRestart = window.confirm(
    `${reason}. Restart the Argentum llama.cpp server now to apply this change?`,
  );
  if (shouldRestart) {
    notify(
      'info',
      'Restarting local server',
      'Stopping and starting llama.cpp with the updated model settings.',
    );
    await runAction('llama-server-stop');
    await runAction('llama-server-start');
    return;
  }

  notify(
    'warning',
    'Restart required',
    'The running llama.cpp server will keep the old model/settings until you restart it.',
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
  requestChatScrollBottom();
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
    requestChatScrollBottom();
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
  requestChatScrollBottom();
  render();
  return block;
}

async function streamProviderAssistantMessage(outgoingMessage, attachments, options = {}) {
  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;
  if (!invoke || typeof listen !== 'function') {
    const result = await sendChatMessage(outgoingMessage, attachments);
    await streamAssistantMessage(result?.message || buildLocalReply(outgoingMessage), options);
    return result;
  }

  const requestId = `stream-${Date.now()}-${(() => {
    const rand = new Uint8Array(8);
    crypto.getRandomValues(rand);
    return [...rand].map((b) => b.toString(16).padStart(2, '0')).join('');
  })()}`;
  const request = await buildChatRequestPayload(outgoingMessage, attachments);
  request.streamRequestId = requestId;

  const block = options.targetId
    ? updateChatMessage(options.targetId, { rawBody: '', status: 'streaming', error: '' })
    : appendChatMessage('argentum', '', { status: 'streaming', id: options.id });
  if (!block) {
    return sendChatMessage(outgoingMessage, attachments);
  }

  let rawBody = '';
  let receivedDelta = false;
  let unlisten = null;
  state.activeAssistantMessageId = block.id;
  state.chatStreaming = true;
  state.chatAbortRequested = false;
  recordUiEvent('chat.response_started', 'running', 'Assistant provider stream started.', {
    chatId: state.activeChatId,
    messageId: block.id,
    requestId,
  });
  requestChatScrollBottom();
  render();

  try {
    unlisten = await listen('argentum-chat-stream', (event) => {
      const payload = event?.payload || {};
      if (payload.requestId !== requestId) return;

      if (payload.event === 'delta') {
        if (state.chatAbortRequested) return;
        const delta = String(payload.delta || '');
        if (!delta) return;
        receivedDelta = true;
        rawBody += delta;
        updateChatMessage(block.id, { rawBody, status: 'streaming' });
        requestChatScrollBottom();
        render();
        return;
      }

      if (payload.event === 'error') {
        const message = normalizeError(payload.message || 'Provider stream failed.');
        updateChatMessage(block.id, {
          rawBody: `Live chat failed: ${message}`,
          status: 'error',
          error: message,
        });
        notify('error', 'Chat failed', message);
        render();
      }
    });

    const result = await invoke('stream_chat_message', { request });
    if (result?.usage) state.usageSnapshot = result.usage;
    const finalMessage = result?.message || rawBody || buildLocalReply(outgoingMessage);
    if (state.chatAbortRequested) {
      updateChatMessage(block.id, {
        rawBody: rawBody || 'Generation stopped.',
        status: 'cancelled',
      });
      recordUiEvent(
        'chat.response_cancelled',
        'cancelled',
        'Assistant provider stream was stopped in the UI.',
        {
          chatId: state.activeChatId,
          messageId: block.id,
          requestId,
        },
      );
      requestChatScrollBottom();
      render();
      return {
        status: 'cancelled',
        message: rawBody || 'Generation stopped.',
        provider: result?.provider || state.llmProvider,
        model: result?.model || state.providerModel,
        offline: false,
        usage: result?.usage,
      };
    }
    if (!receivedDelta) {
      await streamAssistantMessage(finalMessage, { targetId: block.id, delay: 12 });
    } else {
      updateChatMessage(block.id, { rawBody: finalMessage, status: result?.status || 'sent' });
      requestChatScrollBottom();
      render();
    }
    recordUiEvent('chat.response_completed', 'ok', 'Assistant provider stream completed.', {
      chatId: state.activeChatId,
      messageId: block.id,
      requestId,
      provider: result?.provider || state.llmProvider,
      model: result?.model || state.providerModel,
    });
    return result;
  } catch (error) {
    const message = normalizeError(error);
    updateChatMessage(block.id, {
      rawBody: `Live chat failed: ${message}`,
      status: 'error',
      error: message,
    });
    recordUiEvent('chat.response_failed', 'error', message, {
      chatId: state.activeChatId,
      messageId: block.id,
      requestId,
    });
    notify('error', 'Chat failed', message);
    throw error;
  } finally {
    if (typeof unlisten === 'function') unlisten();
    state.chatStreaming = false;
    state.activeAssistantMessageId = '';
    state.chatAbortRequested = false;
    requestChatScrollBottom();
    render();
  }
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

    const result = await streamProviderAssistantMessage(outgoingMessage, attachments, {
      targetId: messageId,
    });
    if (result?.usage) state.usageSnapshot = result.usage;
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
  requestChatScrollBottom(true);
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
  requestChatScrollBottom(true);
  render();

  try {
    const result = await streamProviderAssistantMessage(outgoingMessage, attachments);
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
    const existingError = state.chatBlocks.some(
      (block) => block.role !== 'user' && block.status === 'error' && block.error === message,
    );
    if (!existingError) {
      appendChatMessage('argentum', `Live chat failed: ${message}`, {
        status: 'error',
        error: message,
      });
    }
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
    updateProviderKeyStatus();
  }
  if (target.id === 'provider-custom-model' || target.id === 'settings-provider-custom-model') {
    state.providerModel = target.value.trim();
    state.apiTest = {
      status: 'idle',
      message: 'Model changed. Test the provider before using live chat.',
    };
    return;
  }
  if (target.id === 'settings-llama-model-source') {
    setLlamaServerConfig('modelSource', target.value === 'file' ? 'file' : 'huggingface');
    return;
  }
  if (target.id === 'settings-llama-model-preset') {
    const preset = llamaDownloadPresets.find((candidate) => candidate.id === target.value);
    setLlamaServerConfig('modelPreset', target.value);
    setLlamaServerConfig('modelSource', 'huggingface');
    if (preset) {
      setLlamaServerConfig('hfRepo', preset.repo);
      setLlamaServerConfig('hfFile', preset.file || '');
      if (state.llmProvider === 'llama-cpp') {
        state.providerModel = preset.modelId;
      }
    }
    return;
  }
  if (target.id === 'settings-hf-model-search') {
    state.huggingFaceSearch = {
      ...state.huggingFaceSearch,
      query: target.value,
      error: '',
    };
    return;
  }
  const llamaInputMap = {
    'settings-llama-model-path': ['modelPath', 'string'],
    'settings-llama-hf-repo': ['hfRepo', 'string'],
    'settings-llama-hf-file': ['hfFile', 'string'],
    'settings-llama-host': ['host', 'string'],
    'settings-llama-port': ['port', 'number'],
    'settings-llama-context-size': ['contextSize', 'number'],
    'settings-llama-gpu-layers': ['gpuLayers', 'number'],
    'settings-llama-threads': ['threads', 'number'],
    'settings-llama-temperature': ['temperature', 'float'],
    'settings-llama-top-p': ['topP', 'float'],
    'settings-llama-repeat-penalty': ['repeatPenalty', 'float'],
    'settings-llama-batch-size': ['batchSize', 'number'],
    'settings-llama-ubatch-size': ['ubatchSize', 'number'],
    'settings-llama-parallel-slots': ['parallelSlots', 'number'],
    'settings-llama-cpu-moe': ['cpuMoe', 'number'],
    'settings-llama-timeout': ['timeout', 'number'],
    'settings-llama-cache-type-k': ['cacheTypeK', 'string'],
    'settings-llama-cache-type-v': ['cacheTypeV', 'string'],
  };
  if (Object.prototype.hasOwnProperty.call(llamaInputMap, target.id)) {
    const [key, kind] = llamaInputMap[target.id];
    const raw = target.value;
    const parsed =
      kind === 'string'
        ? raw
        : kind === 'float'
          ? Number.parseFloat(raw)
          : Number.parseInt(raw, 10);
    setLlamaServerConfig(key, Number.isFinite(parsed) ? parsed : raw);
    if (key === 'hfRepo' || key === 'hfFile') {
      setLlamaServerConfig('modelPreset', 'custom');
      setLlamaServerConfig('modelSource', 'huggingface');
    }
    return;
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
    if (target.dataset.contextAccess === 'system-dashboard') {
      if (!target.checked) {
        state.desktopState = { ...state.desktopState, systemStats: null };
      } else if (state.activeSection === 'pc-stats') {
        await refreshSystemDashboardState();
      }
    }
    render();
    return;
  }

  if (target.dataset.llamaBoolean) {
    setLlamaServerConfig(target.dataset.llamaBoolean, target.checked);
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
    const previousModel = state.providerModel;
    if (target.id === 'provider-model') {
      selectModel(target.value);
    } else {
      state.providerModel = target.value;
      state.apiTest = {
        status: 'idle',
        message: 'Model changed. Test the provider before using live chat.',
      };
    }
    if (state.llmProvider === 'llama-cpp') {
      const preset = llamaDownloadPresets.find((candidate) => candidate.modelId === target.value);
      if (preset) {
        setLlamaServerConfig('modelSource', 'huggingface');
        setLlamaServerConfig('modelPreset', preset.id);
        setLlamaServerConfig('hfRepo', preset.repo);
        setLlamaServerConfig('hfFile', preset.file || '');
      }
    }
    if (
      target.id === 'settings-provider-model' &&
      state.llmProvider === 'llama-cpp' &&
      previousModel !== state.providerModel
    ) {
      await promptLlamaServerRestart('Local model changed');
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

  if (target.id === 'settings-accent-custom') {
    setUiPreference('accentColor', target.value.replace('#', ''));
    render();
    return;
  }

  if (target.id === 'settings-high-contrast') {
    state.highContrastMode = target.checked;
    if (state.highContrastMode) {
      document.documentElement.classList.add('high-contrast');
    } else {
      document.documentElement.classList.remove('high-contrast');
    }
    render();
    return;
  }

  if (target.id === 'settings-language') {
    const newLocale = target.value;
    setLocale(newLocale);
    state.uiLanguage = newLocale;
    document.documentElement.dir = textDirection(newLocale);
    document.documentElement.lang = newLocale;
    persistUiPreferences();
    render();
    return;
  }

  if (target.id === 'settings-provider') {
    updateProviderFieldsFromPreset(target.value);
    render();
    return;
  }

  if (target.id === 'settings-llama-model-source') {
    setLlamaServerConfig('modelSource', target.value === 'file' ? 'file' : 'huggingface');
    render();
    return;
  }

  if (target.id === 'settings-llama-model-preset') {
    const previousPreset = state.llamaServerConfig?.modelPreset;
    const preset = llamaDownloadPresets.find((candidate) => candidate.id === target.value);
    setLlamaServerConfig('modelPreset', target.value);
    setLlamaServerConfig('modelSource', 'huggingface');
    if (preset) {
      setLlamaServerConfig('hfRepo', preset.repo);
      setLlamaServerConfig('hfFile', preset.file || '');
      if (state.llmProvider === 'llama-cpp') {
        state.providerModel = preset.modelId;
      }
    }
    if (previousPreset !== target.value) {
      await promptLlamaServerRestart('Local download preset changed');
    }
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
  const isTextInput = target instanceof HTMLTextAreaElement || target instanceof HTMLInputElement;

  // Global keyboard shortcuts (only when not typing in a text field)
  if (!isTextInput || target.id === 'chat-draft') {
    const ctrl = event.ctrlKey || event.metaKey;
    const key = event.key;

    // Close open panels on Escape
    if (key === 'Escape') {
      const anyPanelOpen =
        state.helpOpen ||
        state.notificationsMenuOpen ||
        state.quickSettingsMenuOpen ||
        state.quickSecurityMenuOpen ||
        state.workspaceMenuOpen;
      if (anyPanelOpen) {
        event.preventDefault();
        if (state.helpOpen) toggleHelp(false);
        if (state.notificationsMenuOpen) toggleNotificationsMenu(false);
        if (state.quickSettingsMenuOpen) toggleQuickSettingsMenu(false);
        if (state.quickSecurityMenuOpen) toggleQuickSecurityMenu(false);
        if (state.workspaceMenuOpen) toggleWorkspaceMenu(false);
        render();
        return;
      }
    }

    // Ctrl+, = Settings
    if (ctrl && key === ',') {
      event.preventDefault();
      setActiveSection('settings');
      return;
    }

    // ? = Help (when not in text input)
    if (key === '?' && !isTextInput) {
      event.preventDefault();
      toggleHelp();
      render();
      return;
    }

    // Ctrl+1-9 = Quick navigation to sections
    if (ctrl && key >= '1' && key <= '9') {
      event.preventDefault();
      const sectionOrder = [
        'chat',
        'gateway',
        'local-server',
        'security',
        'pc-stats',
        'settings',
        'diagnostics',
        'logs',
      ];
      const idx = parseInt(key, 10) - 1;
      if (sectionOrder[idx]) {
        setActiveSection(sectionOrder[idx]);
      }
      return;
    }
  }

  // Chat composer: Enter to send (Shift+Enter for newline)
  if (target instanceof HTMLTextAreaElement && target.id === 'chat-draft') {
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

  if (element.closest('#chat-help-button')) {
    toggleHelp();
    render();
    return;
  }

  if (element.closest('[data-close-help]')) {
    toggleHelp(false);
    render();
    return;
  }

  if (element.closest('[data-close-local-server-guide]')) {
    state.localServerGuideOpen = false;
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

  const chatPanelButton = element.closest('[data-toggle-chat-panel]');
  if (chatPanelButton) {
    toggleChatPanel(chatPanelButton.dataset.toggleChatPanel);
    render();
    return;
  }

  if (element.closest('[data-new-transmission]')) {
    requestChatScrollBottom(true);
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

  if (element.closest('#do-migration')) {
    await handleMigrationImport();
    render();
    return;
  }

  if (element.closest('#skip-migration')) {
    state.migrationSkipped = true;
    render();
    return;
  }

  if (element.closest('#choose-llama-model')) {
    await chooseLlamaModelFile();
    render();
    return;
  }

  if (element.closest('#search-hf-models')) {
    await searchHuggingFaceModels();
    return;
  }

  const huggingFaceModel = element.closest('[data-hf-model-repo]');
  if (huggingFaceModel) {
    const repo = huggingFaceModel.dataset.hfModelRepo;
    setLlamaServerConfig('modelSource', 'huggingface');
    setLlamaServerConfig('modelPreset', 'custom');
    setLlamaServerConfig('hfRepo', repo);
    setLlamaServerConfig('hfFile', '');
    if (state.llmProvider === 'llama-cpp') state.providerModel = repo;
    state.apiTest = {
      status: 'idle',
      message: 'Hugging Face model changed. Start llama.cpp, then test the provider.',
    };
    notify('info', 'Hugging Face model selected', repo);
    render();
    return;
  }

  if (element.closest('#scan-local-models')) {
    await scanLocalModels();
    return;
  }

  const localModel = element.closest('[data-local-model-path]');
  if (localModel) {
    const path = localModel.dataset.localModelPath;
    setLlamaServerConfig('modelSource', 'file');
    setLlamaServerConfig('modelPath', path);
    state.providerModel = path.split(/[\\/]/).pop() || path;
    state.apiTest = {
      status: 'idle',
      message: 'Local GGUF model changed. Start llama.cpp, then test the provider.',
    };
    notify('info', 'Local model selected', state.providerModel);
    render();
    return;
  }

  if (element.closest('#choose-user-avatar')) {
    await chooseUserAvatarFile();
    render();
    return;
  }

  if (element.closest('#clear-user-avatar')) {
    clearUserAvatarFile();
    render();
    return;
  }

  if (element.closest('#test-provider')) {
    await testProvider();
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

  const feedbackButton = element.closest('[data-feedback-url]');
  if (feedbackButton) {
    await openExternalUrl(feedbackButton.dataset.feedbackUrl);
    return;
  }

  // Skills tab switching
  const skillsTabBtn = element.closest('[data-skills-tab]');
  if (skillsTabBtn) {
    setSkillsTab(skillsTabBtn.dataset.skillsTab);
    render();
    return;
  }

  // Skills search input
  const skillsSearchInput = element.closest('.skills-search');
  if (skillsSearchInput && element.tagName === 'INPUT') {
    setSkillsSearch(element.value);
    render();
    return;
  }

  // Skills category filter
  const skillsCategoryFilter = element.closest('.skills-category-filter');
  if (skillsCategoryFilter && element.tagName === 'SELECT') {
    setSkillsCategory(element.value);
    render();
    return;
  }

  // Install skill button
  const installBtn = element.closest('.install-skill-btn');
  if (installBtn) {
    const { invoke } = window.__TAURI__?.core || {};
    if (!invoke) {
      notify('error', 'Tauri API not available. Cannot install skill.');
      return;
    }
    installBtn.disabled = true;
    installBtn.textContent = 'Installing…';
    try {
      const result = await invoke('install_skill', {
        source: installBtn.dataset.skillSource,
        skillName: installBtn.dataset.skillName,
      });
      // Refresh installed skills list
      try {
        const installedJson = await invoke('list_installed_skills');
        const installed = JSON.parse(installedJson);
        setInstalledSkills(installed);
      } catch {
        // Best-effort refresh
      }
      notify('success', result);
      recordUiEvent('skills.installed', 'ok', installBtn.dataset.skillName, {
        source: installBtn.dataset.skillSource,
      });
      render();
    } catch (err) {
      notify('error', typeof err === 'string' ? err : String(err));
      installBtn.disabled = false;
      installBtn.textContent = 'Install';
    }
    return;
  }

  // Uninstall skill button
  const uninstallBtn = element.closest('.uninstall-skill-btn');
  if (uninstallBtn) {
    const { invoke } = window.__TAURI__?.core || {};
    if (!invoke) {
      notify('error', 'Tauri API not available. Cannot uninstall skill.');
      return;
    }
    uninstallBtn.disabled = true;
    uninstallBtn.textContent = 'Removing…';
    try {
      const result = await invoke('uninstall_skill', {
        skillName: uninstallBtn.dataset.skillName,
      });
      // Refresh installed skills list
      try {
        const installedJson = await invoke('list_installed_skills');
        const installed = JSON.parse(installedJson);
        setInstalledSkills(installed);
      } catch {
        // Best-effort refresh
      }
      notify('success', result);
      recordUiEvent('skills.uninstalled', 'ok', uninstallBtn.dataset.skillName);
      render();
    } catch (err) {
      notify('error', typeof err === 'string' ? err : String(err));
      uninstallBtn.disabled = false;
      uninstallBtn.textContent = 'Uninstall';
    }
    return;
  }

  const accentSwatch = element.closest('[data-accent-color]');
  if (accentSwatch) {
    setUiPreference('accentColor', accentSwatch.dataset.accentColor);
    // Sync the color input value
    const colorInput = document.getElementById('settings-accent-custom');
    if (colorInput) colorInput.value = '#' + accentSwatch.dataset.accentColor;
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
    requestChatScrollBottom(true);
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

  if (element.closest('#check-for-updates')) {
    await checkForUpdates();
    return;
  }

  if (element.closest('#download-update')) {
    await downloadUpdate();
    return;
  }

  if (element.closest('#settings-rescan-migration')) {
    await detectMigrationSources();
    render();
    return;
  }

  if (element.closest('#settings-do-migration')) {
    await handleMigrationImport();
    render();
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
    requestChatScrollBottom(true);
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

function handleContextMenu(event) {
  const card =
    event.target instanceof Element ? event.target.closest('[data-conversation-card]') : null;
  if (!card) return;
  event.preventDefault();
  toggleConversationMenu(card.dataset.conversationCard);
  render();
}

addActivationListeners(document);
addActivationListeners(document.body);
addActivationListeners(viewRoot);
addActivationListeners(overlayRoot);
document.addEventListener('input', handleInput);
document.addEventListener('change', handleChange);
document.addEventListener('keydown', handleKeyDown);
document.addEventListener('contextmenu', handleContextMenu);
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

// Initialize skills catalog with static Anthropic + Codex data
setSkillsCatalog(ANTHROPIC_SKILLS, [...CODEX_CURATED_SKILLS, ...CODEX_SYSTEM_SKILLS]);

// Apply locale and text direction from persisted preference
setLocale(state.uiLanguage);
document.documentElement.dir = textDirection();
document.documentElement.lang = state.uiLanguage;

// Re-apply locale + RTL on locale change and re-render
document.addEventListener('localechange', () => {
  document.documentElement.dir = textDirection();
  document.documentElement.lang = state.uiLanguage;
  render();
});
hydrateDesktopDefaults()
  .then(() => refreshDesktopState())
  .then(() => {
    // Load installed skills list on startup
    const { invoke } = window.__TAURI__?.core || {};
    if (invoke) {
      invoke('list_installed_skills')
        .then((json) => {
          try {
            setInstalledSkills(JSON.parse(json));
          } catch {
            // Ignore parse errors
          }
        })
        .catch(() => {
          // Best-effort load
        });
    }
  })
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
