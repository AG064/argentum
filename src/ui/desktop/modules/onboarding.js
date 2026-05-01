import {
  channelOptions,
  experienceLevels,
  onboardingSteps,
  providerPresets,
  runtimeModes,
  securityProfiles,
} from './constants.js';
import { renderNotifications } from './shell.js';
import { state } from './state.js';
import {
  buttonDisabled,
  checked,
  currentProvider,
  escapeAttribute,
  escapeHtml,
  explainPath,
  labelFor,
  selected,
} from './utils.js';

export const onboardingModule = {
  id: 'onboarding',
  label: 'Onboarding',
  render: renderOnboarding,
  validate: validateCurrentStep,
  healthCheck: () => ({
    status: state.setupComplete ? 'ok' : 'pending',
    message: state.setupComplete ? 'Setup complete' : 'Setup is still in progress',
  }),
};

export function validateCurrentStep() {
  if (state.onboardingStep === 2 && !state.workspacePath.trim()) {
    return 'Choose a workspace folder before continuing.';
  }

  if (state.onboardingStep === 5 && !state.providerBaseUrl.trim()) {
    return 'Add the provider endpoint before continuing.';
  }

  return '';
}

function renderOnboarding() {
  const currentStep = onboardingSteps[state.onboardingStep - 1];
  const isFinalStep = state.onboardingStep === onboardingSteps.length;

  return `
    ${renderNotifications()}
    <div class="onboarding-layout">
      <section class="panel onboarding-panel">
        <div class="panel-header">
          <span class="pill">Step ${state.onboardingStep} of ${onboardingSteps.length}</span>
          <h2>${escapeHtml(currentStep)}</h2>
          <p>${escapeHtml(stepIntro())}</p>
        </div>
        <div class="panel-body">
          ${renderOnboardingStep()}
          <div class="button-row">
            <button class="button" id="back-button" ${buttonDisabled(state.onboardingStep === 1)}>Back</button>
            <button class="button primary" id="next-button">${isFinalStep ? 'Launch Argentum' : 'Next'}</button>
          </div>
        </div>
      </section>
      <aside class="panel setup-progress">
        <div class="panel-header">
          <h3>Setup Flow</h3>
          <p>Every choice stays editable until launch.</p>
        </div>
        <div class="panel-body step-list">
          ${onboardingSteps
            .map(
              (step, index) => `
                <button class="step-card ${index + 1 === state.onboardingStep ? 'active' : ''}" data-onboarding-step="${index + 1}">
                  <span>Step ${index + 1}</span>
                  <strong>${escapeHtml(step)}</strong>
                </button>
              `,
            )
            .join('')}
        </div>
      </aside>
    </div>
  `;
}

function stepIntro() {
  const workspace = explainPath(state.workspacePath);
  const intros = {
    1: 'Argentum is a local-first agent workspace. It starts small, explains what it can reach, and asks before expanding access.',
    2: `Pick the folder Argentum is allowed to use by default. Default access means all folders and files inside ${workspace}.`,
    3: 'Choose how you want to run Argentum day to day. You can keep the CLI while making the desktop app the main interface.',
    4: 'Capabilities are the boundaries. Argentum can suggest powerful actions, but approval decides what actually runs.',
    5: 'Choose a model provider. Hosted providers need keys; local providers usually only need a reachable endpoint.',
    6: 'Choose every channel you want. Local app stays on; everything else is optional.',
    7: 'Pick how cautious Argentum should be when tools, files, shell, network, or integrations are requested.',
    8: 'Review the setup and test the provider. If the test fails, Argentum can still open in offline guided mode.',
    9: 'Setup will save config, hide onboarding, and open Chat for the introductory phase.',
  };
  return intros[state.onboardingStep] || '';
}

function renderOnboardingStep() {
  const renderers = {
    1: renderWelcomeStep,
    2: renderWorkspaceStep,
    3: renderRuntimeStep,
    4: renderCapabilitiesStep,
    5: renderProviderStep,
    6: renderChannelsStep,
    7: renderSecurityStep,
    8: renderReviewStep,
    9: renderFinishStep,
  };
  return renderers[state.onboardingStep]();
}

function renderWelcomeStep() {
  return `
    <div class="copy-block">
      <h3>What Argentum is</h3>
      <p>Argentum is a modular AI workspace for chat, agents, skills, memory, integrations, and local automation. The first version is intentionally local-first: it should be useful before you connect outside services.</p>
      <p><strong>What it can access by default:</strong> all the folders/files inside the workspace folder you choose. It should not wander into Downloads, Desktop, browser data, RAM, or other apps unless you explicitly grant a capability later.</p>
    </div>
    <div class="interface-grid">
      ${experienceLevels
        .map(
          (level) => `
            <button class="interface-card ${state.experienceLevel === level.id ? 'active' : ''}" data-experience-level="${level.id}">
              <span>${escapeHtml(level.label)}</span>
              <strong>${escapeHtml(level.headline)}</strong>
              <p>${escapeHtml(level.detail)}</p>
            </button>
          `,
        )
        .join('')}
    </div>
  `;
}

function renderWorkspaceStep() {
  return `
    <div class="workspace-picker">
      <label>
        Workspace folder
        <div class="input-with-button">
          <input id="workspace-input" value="${escapeAttribute(state.workspacePath)}" aria-describedby="workspace-help" readonly />
          <button class="button" id="choose-workspace">Browse...</button>
        </div>
      </label>
      <p id="workspace-help">Default access: all the folders/files in workspace folder <strong>${escapeHtml(state.workspacePath)}</strong>.</p>
      <div class="plain-warning">
        Argentum config, data, logs, cache, and local memory will be stored here for now. More data modes can be added later.
      </div>
    </div>
  `;
}

function renderRuntimeStep() {
  return `
    <div class="interface-grid">
      ${runtimeModes
        .map(
          (mode) => `
            <button class="interface-card ${state.runtimeMode === mode.id ? 'active' : ''}" data-runtime-mode="${mode.id}">
              <span>${escapeHtml(mode.label)}</span>
              <strong>${escapeHtml(mode.headline)}</strong>
              <p>${escapeHtml(mode.detail)}</p>
            </button>
          `,
        )
        .join('')}
    </div>
    <div class="demo-panel">
      <span>Preview</span>
      <strong>${escapeHtml(runtimeModes.find((mode) => mode.id === state.runtimeMode)?.headline || 'Runtime')}</strong>
      <p>${escapeHtml(runtimeModes.find((mode) => mode.id === state.runtimeMode)?.preview || '')}</p>
    </div>
  `;
}

function renderCapabilitiesStep() {
  return `
    <div class="capability-grid">
      ${[
        ['Workspace files', `Allowed by default only inside ${state.workspacePath}.`],
        ['Shell commands', 'Ask before execution. The request should show command, folder, and reason.'],
        ['Network access', 'Ask before contacting external APIs, websites, or integrations.'],
        ['Self-repair', 'May suggest fixes, but repair actions need approval and stay inside approved folders.'],
        ['Secrets', 'API keys are kept outside YAML and hidden after entry.'],
        ['Audit trail', 'Permission decisions and sensitive actions are written to the audit log.'],
      ]
        .map(
          ([title, detail]) => `
            <article class="feature-card">
              <span class="pill">Capability</span>
              <h3>${escapeHtml(title)}</h3>
              <p>${escapeHtml(detail)}</p>
            </article>
          `,
        )
        .join('')}
    </div>
  `;
}

function renderProviderStep() {
  const provider = currentProvider(providerPresets, state);
  return `
    <div class="provider-layout">
      <div class="provider-list">
        ${providerPresets
          .map(
            (item) => `
              <button class="provider-card ${state.llmProvider === item.id ? 'active' : ''}" data-provider-id="${item.id}">
                <strong>${escapeHtml(item.label)}</strong>
                <span>${item.requiresKey ? 'API key needed' : 'Local/custom friendly'}</span>
                <p>${escapeHtml(item.detail)}</p>
              </button>
            `,
          )
          .join('')}
      </div>
      <div class="panel provider-settings">
        <div class="panel-body form-grid">
          <label>
            Endpoint
            <input id="provider-base-url" value="${escapeAttribute(state.providerBaseUrl)}" placeholder="${escapeAttribute(provider.defaultBaseUrl)}" />
          </label>
          <label>
            Model
            <input id="provider-model" value="${escapeAttribute(state.providerModel)}" placeholder="${escapeAttribute(provider.defaultModel)}" />
          </label>
          <label>
            API style
            <select id="provider-api">
              <option value="openai" ${selected(state.providerApi, 'openai')}>OpenAI-compatible</option>
              <option value="anthropic" ${selected(state.providerApi, 'anthropic')}>Anthropic-compatible</option>
            </select>
          </label>
          <label>
            API key
            <input id="provider-api-key" type="password" value="${escapeAttribute(state.providerApiKey)}" placeholder="${provider.requiresKey ? 'Required for this provider' : 'Optional for local/custom'}" autocomplete="new-password" />
          </label>
          ${
            state.llmProvider === 'custom'
              ? `
                <label>
                  Provider name
                  <input id="custom-provider-name" value="${escapeAttribute(state.customProviderName)}" />
                </label>
                <label>
                  Secret variable name
                  <input id="custom-api-key-env" value="${escapeAttribute(state.customApiKeyEnv)}" />
                </label>
              `
              : ''
          }
          <div class="test-row">
            <button class="button" id="test-provider">Test API</button>
            <span class="pill ${providerTestClass()}">${escapeHtml(providerTestLabel())}</span>
            <p>${escapeHtml(state.apiTest.message)}</p>
          </div>
        </div>
      </div>
    </div>
  `;
}

function renderChannelsStep() {
  return `
    <div class="channel-grid">
      ${channelOptions
        .map(
          (channel) => `
            <label class="check-card ${state.selectedChannels.includes(channel.id) ? 'active' : ''}">
              <input type="checkbox" data-channel-id="${channel.id}" ${checked(state.selectedChannels, channel.id)} ${channel.locked ? 'disabled' : ''} />
              <span>${escapeHtml(channel.status)}</span>
              <strong>${escapeHtml(channel.label)}</strong>
              <p>${escapeHtml(channel.detail)}</p>
            </label>
          `,
        )
        .join('')}
    </div>
    <div class="form-grid two channel-settings">
      <label>
        Webchat token
        <input id="webchat-token" type="password" value="${escapeAttribute(state.webchatToken)}" placeholder="Generated if left empty" autocomplete="new-password" />
      </label>
      <label>
        Telegram bot token
        <input id="telegram-token" type="password" value="${escapeAttribute(state.telegramToken)}" placeholder="Only needed if Telegram is selected" autocomplete="new-password" />
      </label>
      <label>
        Telegram users/chats
        <input id="telegram-allowlist" value="${escapeAttribute(state.telegramAllowlist)}" placeholder="Comma-separated user or chat IDs" />
      </label>
      <label>
        WhatsApp phone ID
        <input id="whatsapp-phone-id" value="${escapeAttribute(state.whatsappPhoneId)}" placeholder="Advanced: Business API setup" />
      </label>
    </div>
  `;
}

function renderSecurityStep() {
  return `
    <div class="security-choice-grid">
      ${securityProfiles
        .map(
          (profile) => `
            <button class="interface-card ${state.securityProfile === profile.id ? 'active' : ''}" data-security-profile="${profile.id}">
              <span>${escapeHtml(profile.label)}</span>
              <strong>${escapeHtml(profile.id === 'restricted' ? 'Best first choice' : profile.label)}</strong>
              <p>${escapeHtml(profile.detail)}</p>
            </button>
          `,
        )
        .join('')}
    </div>
  `;
}

function renderReviewStep() {
  return `
    ${renderReviewRows()}
    <div class="button-row split">
      <button class="button" data-onboarding-step="5">Edit provider</button>
      <button class="button primary" id="test-provider">Test API again</button>
    </div>
  `;
}

function renderFinishStep() {
  return `
    <div class="finish-panel">
      <div class="launch-animation ${state.setupAnimation ? 'running' : ''}">
        <span></span><span></span><span></span>
      </div>
      <h3>Ready to enter Chat</h3>
      <p>Argentum will save config under the workspace, keep secrets outside YAML, hide onboarding, and begin the guided introductory chat.</p>
      ${renderReviewRows()}
    </div>
  `;
}

function renderReviewRows() {
  const provider = currentProvider(providerPresets, state);
  const channels = state.selectedChannels
    .map((channel) => labelFor(channelOptions, channel))
    .join(', ');

  return `
    <div class="status-list">
      <div class="status-row"><strong>Workspace</strong><span>${escapeHtml(state.workspacePath)}</span><span class="pill ok">Scoped</span></div>
      <div class="status-row"><strong>Experience</strong><span>${escapeHtml(labelFor(experienceLevels, state.experienceLevel))}</span><span class="pill ok">Set</span></div>
      <div class="status-row"><strong>Runtime</strong><span>${escapeHtml(labelFor(runtimeModes, state.runtimeMode))}</span><span class="pill ok">Set</span></div>
      <div class="status-row"><strong>Provider</strong><span>${escapeHtml(provider.label)} - ${escapeHtml(state.providerModel)}</span><span class="pill ${providerTestClass()}">${escapeHtml(providerTestLabel())}</span></div>
      <div class="status-row"><strong>Channels</strong><span>${escapeHtml(channels)}</span><span class="pill ok">Selected</span></div>
      <div class="status-row"><strong>Security</strong><span>${escapeHtml(labelFor(securityProfiles, state.securityProfile))}</span><span class="pill ok">Audited</span></div>
      <div class="status-row"><strong>Config</strong><span>Saved to config/default.yaml. Secrets saved separately.</span><span class="pill warn">Pending save</span></div>
    </div>
  `;
}

function providerTestClass() {
  if (state.apiTest.status === 'ok') return 'ok';
  if (state.apiTest.status === 'error') return 'danger';
  return 'warn';
}

function providerTestLabel() {
  if (state.apiTest.status === 'ok') return 'Test passed';
  if (state.apiTest.status === 'testing') return 'Testing';
  if (state.apiTest.status === 'error') return 'Check needed';
  return 'Not tested';
}
