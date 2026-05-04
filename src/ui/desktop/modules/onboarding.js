import {
  channelOptions,
  experienceLevels,
  onboardingSteps,
  providerAuthMethods,
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
  modelOptionsFor,
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

  if (state.onboardingStep === 5) {
    const provider = currentProvider(providerPresets, state);
    if (!state.providerSelectionConfirmed) {
      return 'Choose an AI provider before continuing.';
    }

    if (state.providerSetupStage !== 'model') {
      return 'Finish the provider setup steps before continuing.';
    }

    if (state.providerAuthMethod === 'api-key' && provider.requiresKey && !state.providerApiKey.trim()) {
      return 'Add an API key or choose browser account authorization before continuing.';
    }

    if (state.providerAuthMethod === 'browser-account' && state.codexOAuth.status !== 'ok') {
      return 'Complete OpenAI/Codex authorization before continuing with browser account auth.';
    }
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
  const availableAuthMethods = providerAuthMethods.filter((method) =>
    (provider.authMethods || ['api-key']).includes(method.id),
  );

  if (!state.providerSelectionConfirmed || state.providerSetupStage === 'provider') {
    return renderProviderChoiceStep();
  }

  if (state.providerSetupStage === 'auth') {
    return renderProviderAuthStep(provider, availableAuthMethods);
  }

  if (state.providerSetupStage === 'credentials') {
    return renderProviderCredentialStep(provider);
  }

  return renderProviderModelStep(provider, availableAuthMethods);
}

function renderProviderChoiceStep() {
  return `
    ${renderProgressiveProviderFrame(
      'Provider',
      'Choose who runs the model',
      '<p class="muted-line">Pick one provider first. Authorization, model choice, endpoint, and tests appear after this choice so this screen stays readable.</p>',
    )}
    <div class="provider-stage">
      <div class="provider-list provider-choice-list">
        ${providerPresets
          .map(
            (item) => `
              <article class="provider-card ${state.llmProvider === item.id ? 'active' : ''}">
                <button class="provider-select-button" data-provider-id="${item.id}">
                  <strong>${escapeHtml(item.label)}</strong>
                  <span>${item.requiresKey ? 'Hosted provider' : 'Local/custom friendly'}</span>
                </button>
                <a class="provider-website-link" href="${escapeAttribute(item.websiteUrl)}" data-open-external="${escapeAttribute(item.websiteUrl)}">
                  Provider website
                  <span data-icon="externalLink"></span>
                </a>
              </article>
            `,
          )
          .join('')}
      </div>
    </div>
  `;
}

function renderProviderAuthStep(provider, availableAuthMethods) {
  return `
    <div class="provider-stage">
      <div class="provider-stage-header split-header">
        <div>
          <span class="pill">Authorization</span>
          <h3>${escapeHtml(provider.label)}</h3>
        </div>
        <button class="button" data-provider-setup-stage="provider">Change provider</button>
      </div>
      <div class="provider-focus-panel">
        <label class="compact-selector">
          Authorization method
          <select id="provider-auth-method">
            ${availableAuthMethods
              .map(
                (method) => `
                  <option value="${escapeAttribute(method.id)}" ${selected(state.providerAuthMethod, method.id)} ${method.disabled ? 'disabled' : ''}>${escapeHtml(method.label)}</option>
                `,
              )
              .join('')}
          </select>
        </label>
        <div class="auth-method-grid">
          ${availableAuthMethods
            .map(
              (method) => `
                <button class="interface-card ${state.providerAuthMethod === method.id ? 'active' : ''}" data-provider-auth-method="${escapeAttribute(method.id)}" data-provider-setup-stage="credentials">
                  <span>${escapeHtml(method.status)}</span>
                  <strong>${escapeHtml(method.label)}</strong>
                  <p>${escapeHtml(method.detail)}</p>
                </button>
              `,
            )
            .join('')}
        </div>
      </div>
    </div>
  `;
}

function renderProviderCredentialStep(provider) {
  if (state.providerAuthMethod === 'browser-account') {
    return `
      <div class="provider-stage">
        <div class="provider-stage-header split-header">
          <div>
            <span class="pill">Authorize</span>
            <h3>${escapeHtml(provider.label)} browser account</h3>
          </div>
          <button class="button" data-provider-setup-stage="auth">Back</button>
        </div>
        <div class="provider-focus-panel wide">
          ${renderProviderAuthGuidance(provider)}
          ${renderCodexOAuthPanel()}
        </div>
      </div>
    `;
  }

  return `
    <div class="provider-stage">
      <div class="provider-stage-header split-header">
        <div>
          <span class="pill">API key</span>
          <h3>${escapeHtml(provider.label)}</h3>
        </div>
        <button class="button" data-provider-setup-stage="auth">Back</button>
      </div>
      <div class="provider-focus-panel">
        <div class="form-grid">
          <label>
            API key
            <input id="provider-api-key" type="password" value="${escapeAttribute(state.providerApiKey)}" placeholder="${provider.requiresKey ? 'Required for this provider' : 'Optional for local/custom'}" autocomplete="new-password" />
          </label>
          ${renderProviderAuthGuidance(provider)}
          <div class="button-row split">
            <span class="pill ${provider.requiresKey ? 'warn' : 'ok'}">${provider.requiresKey ? 'Key required' : 'Key optional'}</span>
            <button class="button primary" id="continue-provider-model" data-provider-setup-stage="model">Continue to model</button>
          </div>
        </div>
      </div>
    </div>
  `;
}

function renderProviderModelStep(provider) {
  return `
    <div class="provider-stage">
      <div class="provider-stage-header split-header">
        <div>
          <span class="pill">Model</span>
          <h3>${escapeHtml(provider.label)}</h3>
        </div>
        <button class="button" data-provider-setup-stage="auth">Change authorization</button>
      </div>
      <div class="provider-focus-panel wide">
        <div class="form-grid two">
          <label>
            Model
            <select id="provider-model">
              ${modelOptionsFor(provider, state.providerModel, state.providerAuthMethod)
                .map(
                  (model) => `
                    <option value="${escapeAttribute(model.id)}" ${selected(state.providerModel, model.id)} data-model-auth-methods="${escapeAttribute(state.providerAuthMethod)}">${escapeHtml(model.label || model.id)}</option>
                  `,
                )
                .join('')}
            </select>
          </label>
          <label>
            Endpoint
            <input id="provider-base-url" value="${escapeAttribute(state.providerBaseUrl)}" placeholder="${escapeAttribute(provider.defaultBaseUrl)}" />
          </label>
          <label>
            API style
            <select id="provider-api">
              <option value="openai" ${selected(state.providerApi, 'openai')}>OpenAI-compatible</option>
              <option value="anthropic" ${selected(state.providerApi, 'anthropic')}>Anthropic-compatible</option>
            </select>
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
            <button class="button" id="test-provider">Test Provider</button>
            <span class="pill ${providerTestClass()}">${escapeHtml(providerTestLabel())}</span>
            <p>${escapeHtml(state.apiTest.message)}</p>
          </div>
        </div>
      </div>
    </div>
  `;
}

function renderProgressiveProviderFrame(stage, title, body) {
  return `
    <div class="provider-stage-header">
      <span class="pill">${escapeHtml(stage)}</span>
      <h3>${escapeHtml(title)}</h3>
      ${body}
    </div>
  `;
}

function renderProviderAuthGuidance(provider) {
  if (state.providerAuthMethod === 'browser-account') {
    return `
      <div class="setup-guidance">
        <span class="pill">Step-by-step</span>
        <h3>Authorize with your OpenAI account</h3>
        <ol>
          <li>Start OpenAI/Codex authorization to create a one-time browser code.</li>
          <li>Open the verification page with the external-link button.</li>
          <li>Approve Argentum in your browser, then come back here.</li>
          <li>Complete authorization, then choose a model and run Test Provider.</li>
        </ol>
      </div>
    `;
  }

  return `
    <div class="setup-guidance">
      <span class="pill">Step-by-step</span>
      <h3>Use a ${escapeHtml(provider.label)} API key</h3>
      <ol>
        <li>Open the provider website and create or copy an API key.</li>
        <li>Paste the key in this setup screen. Password fields stay hidden.</li>
        <li>Pick a model and endpoint, then run Test Provider.</li>
        <li>Finish onboarding after the test passes or continue in offline mode.</li>
      </ol>
    </div>
  `;
}

function renderCodexOAuthPanel() {
  const oauth = state.codexOAuth || {};
  const isBrowserAuth = state.providerAuthMethod === 'browser-account';
  const verificationUrl = oauth.verificationUrl || 'https://auth.openai.com/codex/device';
  const canUseOAuth = (currentProvider(providerPresets, state).authMethods || []).includes('browser-account');

  if (!canUseOAuth) return '';

  return `
    <div class="oauth-panel ${isBrowserAuth ? 'active' : ''}">
      <div>
        <span class="pill">${isBrowserAuth ? 'Selected' : 'Optional'}</span>
        <h3>OpenAI/Codex authorization</h3>
        <p>Choose browser account authorization, start the flow, open the verification page, then enter the code shown here. Credentials stay inside the selected workspace folder.</p>
      </div>
      <div class="oauth-actions">
        <button class="button" id="start-codex-oauth">Start OpenAI/Codex authorization</button>
        <button class="button primary" id="complete-codex-oauth">Complete authorization</button>
      </div>
      <div class="oauth-code-grid">
        <div>
          <span>Verification page</span>
          <a href="${escapeAttribute(verificationUrl)}" data-open-external="${escapeAttribute(verificationUrl)}">
            ${escapeHtml(verificationUrl)}
            <span data-icon="externalLink"></span>
          </a>
        </div>
        <div>
          <span>User code</span>
          <strong>${escapeHtml(oauth.userCode || 'Start authorization first')}</strong>
        </div>
        <div>
          <span>Status</span>
          <strong>${escapeHtml(oauth.status || 'idle')}</strong>
        </div>
      </div>
      <p>${escapeHtml(oauth.message || 'OpenAI/Codex authorization has not been started.')}</p>
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
