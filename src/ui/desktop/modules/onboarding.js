import {
  channelOptions,
  experienceLevels,
  onboardingSteps,
  providerPresets,
  runtimeModes,
  securityProfiles,
} from './constants.js';
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
  const canCancel = state.setupComplete;

  return `
    <div class="onboarding-backdrop" role="presentation">
      <section class="onboarding-modal" role="dialog" aria-modal="true" aria-labelledby="onboarding-title">
        <div class="onboarding-modal-header">
          <div>
            <span class="pill">Step ${state.onboardingStep} of ${onboardingSteps.length}</span>
            <h2 id="onboarding-title">${escapeHtml(currentStep)}</h2>
            <p>${escapeHtml(stepIntro())}</p>
          </div>
          ${canCancel ? '<button class="icon-button" data-cancel-onboarding="true" aria-label="Close onboarding">x</button>' : ''}
        </div>
        <div class="onboarding-progress" aria-label="Setup progress">
          ${onboardingSteps
            .map(
              (step, index) => `
                <button class="step-chip ${index + 1 === state.onboardingStep ? 'active' : ''}" data-onboarding-step="${index + 1}">
                  <span>${index + 1}</span>
                  <strong>${escapeHtml(step)}</strong>
                </button>
              `,
            )
            .join('')}
        </div>
        <div class="onboarding-modal-body">
          ${renderOnboardingStep()}
        </div>
        <div class="onboarding-modal-footer">
          ${canCancel ? '<button class="button" data-cancel-onboarding="true">Cancel</button>' : '<span></span>'}
          <div class="button-row">
            <button class="button" id="back-button" ${buttonDisabled(state.onboardingStep === 1)}>Back</button>
            <button class="button primary" id="next-button">${isFinalStep ? 'Save and open Argentum' : 'Next'}</button>
          </div>
        </div>
      </section>
    </div>
  `;
}

function stepIntro() {
  const workspace = explainPath(state.workspacePath);
  const intros = {
    1: 'Meet the project, then choose how much guidance you want.',
    2: `Pick the one folder Argentum can use by default: ${workspace}.`,
    3: 'Choose the main way you want to work and see a concrete example.',
    4: 'Review what Argentum can request and what stays blocked until you approve it.',
    5: 'Choose hosted, local, or custom model access. Failed tests do not block offline mode.',
    6: 'Enable the surfaces you actually want. Local app stays on.',
    7: 'Choose how strict approvals should be before tools, files, shell, or network run.',
    8: 'Review the choices, test the provider, then open Chat.',
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
  };
  return renderers[state.onboardingStep]();
}

function renderWelcomeStep() {
  return `
    <div class="copy-block compact-copy">
      <h3>What Argentum is</h3>
      <p>Argentum is a local-first workspace for chat, agents, skills, memory, channels, and controlled automation.</p>
      <p><strong>Default access: all folders/files inside workspace folder.</strong> It should not read Downloads, Desktop, browser data, RAM, or other apps unless you approve a capability later.</p>
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
  const mode = runtimeModes.find((item) => item.id === state.runtimeMode) || runtimeModes[0];
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
    ${renderRuntimeDemo(mode)}
  `;
}

function renderRuntimeDemo(mode) {
  return `
    <div class="runtime-demo">
      <div>
        <span>Example flow</span>
        <strong>${escapeHtml(mode.headline)}</strong>
        <p>${escapeHtml(mode.preview)}</p>
      </div>
      <div class="demo-stage" aria-hidden="true">
        ${mode.demoSteps
          .map(
            (step, index) => `
              <div class="demo-node" data-demo-step="${index + 1}">
                <span>${index + 1}</span>
                <strong>${escapeHtml(step)}</strong>
              </div>
            `,
          )
          .join('')}
      </div>
      <div class="demo-examples">
        ${mode.examples
          .map(
            (example) => `
              <div>
                <span></span>
                <p>${escapeHtml(example)}</p>
              </div>
            `,
          )
          .join('')}
      </div>
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
    <div class="review-pipeline" aria-label="Review test pass">
      <div><span>Review</span><strong>Choices</strong></div>
      <div><span>Test</span><strong>${escapeHtml(providerTestLabel())}</strong></div>
      <div><span>Pass</span><strong>Open Chat</strong></div>
    </div>
    ${renderReviewRows()}
    <div class="button-row split">
      <button class="button" data-onboarding-step="5">Edit provider</button>
      <button class="button primary" id="test-provider">Test API</button>
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
