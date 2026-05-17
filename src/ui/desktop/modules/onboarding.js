import {
  channelOptions,
  contextAccessOptions,
  experienceLevels,
  modelMetadata,
  onboardingSteps,
  providerAuthMethods,
  providerCatalogTabs,
  providerPresets,
  runtimeModes,
  securityProfiles,
} from './constants.js';
import { state } from './state.js';
import { validateCurrentStep } from './onboarding-controller.js';
import {
  buttonDisabled,
  checked,
  currentProvider,
  escapeAttribute,
  escapeHtml,
  explainPath,
  labelFor,
  modelMetadataFor,
  modelOptionsFor,
  selected,
} from './utils.js';

export const onboardingModule = {
  id: 'onboarding',
  label: 'Onboarding',
  render: renderOnboarding,
  validate: () => validateCurrentStep().message,
  healthCheck: () => ({
    status: state.setupComplete ? 'ok' : 'pending',
    message: state.setupComplete ? 'Setup complete' : 'Setup is still in progress',
  }),
};

function renderOnboarding() {
  const currentStep = onboardingSteps[state.onboardingStep - 1];
  const isFinalStep = state.onboardingStep === onboardingSteps.length;
  const canCancel = state.setupComplete;

  return `
    <div class="onboarding-backdrop" role="presentation">
      <section class="onboarding-modal onboarding-wizard" role="dialog" aria-modal="true" aria-labelledby="onboarding-title">
        <div class="onboarding-modal-header">
          <div>
            <span class="pill">Step ${state.onboardingStep} of ${onboardingSteps.length}</span>
            <h2 id="onboarding-title">${escapeHtml(currentStep)}</h2>
            <p>${escapeHtml(stepIntro())}</p>
          </div>
          ${canCancel ? '<button type="button" class="icon-button" data-cancel-onboarding="true" aria-label="Close onboarding">x</button>' : ''}
        </div>
        <div class="onboarding-modal-body onboarding-step-layout">
          <nav class="onboarding-progress onboarding-step-list" aria-label="Setup progress">
            ${onboardingSteps
              .map(
                (step, index) => `
                  <button type="button" class="step-chip ${index + 1 === state.onboardingStep ? 'active' : ''}" data-onboarding-step="${index + 1}">
                    <span>${index + 1}</span>
                    <strong>${escapeHtml(step)}</strong>
                  </button>
                `,
              )
              .join('')}
          </nav>
          <div class="onboarding-step-panel">
            ${renderValidationError()}
            ${renderOnboardingStep()}
          </div>
        </div>
        <div class="onboarding-modal-footer">
          ${canCancel ? '<button type="button" class="button" data-cancel-onboarding="true">Cancel</button>' : '<span></span>'}
          <div class="button-row">
            <button type="button" class="button" id="back-button" ${buttonDisabled(state.onboardingStep === 1)}>Back</button>
            <button type="button" class="button primary" id="next-button">${isFinalStep ? 'Save and open Argentum' : 'Next'}</button>
          </div>
        </div>
      </section>
    </div>
  `;
}

function renderValidationError() {
  if (!state.onboardingError) return '';

  return `
    <div class="onboarding-validation" role="alert">
      <strong>Setup needs a fix</strong>
      <p>${escapeHtml(state.onboardingError)}</p>
    </div>
  `;
}

function stepIntro() {
  const workspace = explainPath(state.workspacePath);
  const intros = {
    1: 'Choose how guided setup should feel.',
    2: `Pick the only folder Argentum can use by default: ${workspace}.`,
    3: 'Choose one model provider. Nothing is selected until you click.',
    4: 'Add access details for the selected provider.',
    5: 'Choose the model for this workspace.',
    6: 'Set the names, tone, context, and optional channels.',
    7: 'Review the essentials, test access, then open Chat.',
  };
  return intros[state.onboardingStep] || '';
}

function renderOnboardingStep() {
  const renderers = {
    1: renderWelcomeStep,
    2: renderWorkspaceStep,
    3: renderProviderChoiceStep,
    4: renderProviderCredentialStep,
    5: renderProviderModelStep,
    6: renderBehaviorStep,
    7: renderTestAndStartStep,
  };
  return renderers[state.onboardingStep]();
}

function renderWelcomeStep() {
  return `
    <div class="onboarding-focus-card">
      <span class="pill ok">Local-first</span>
      <h3>What Argentum is</h3>
      <p>Argentum is a desktop AI workspace for chat, controlled automation, providers, channels, logs, and diagnostics.</p>
      <p><strong>Default access: all folders/files inside workspace folder.</strong> Other folders, apps, browser data, RAM, and shell actions need explicit permission.</p>
    </div>
    <div class="interface-grid">
      ${experienceLevels
        .map(
          (level) => `
            <button type="button" class="interface-card ${state.experienceLevel === level.id ? 'active' : ''}" data-experience-level="${level.id}">
              <span>${escapeHtml(level.label)}</span>
              <strong>${escapeHtml(level.headline)}</strong>
              <p>${escapeHtml(experienceShortHint(level.id))}</p>
            </button>
          `,
        )
        .join('')}
    </div>
  `;
}

function experienceShortHint(levelId) {
  const hints = {
    beginner: 'Plain language and safer defaults.',
    comfortable: 'Recommended balance.',
    expert: 'Compact setup with advanced controls.',
  };
  return hints[levelId] || 'Guided setup.';
}

function renderWorkspaceStep() {
  const hasNativeFolderPicker = typeof window !== 'undefined' && Boolean(window.__TAURI__?.dialog?.open);
  return `
    <div class="workspace-picker onboarding-compact-stack">
      <label>
        Workspace folder
        <div class="input-with-button">
          <input id="workspace-input" value="${escapeAttribute(state.workspacePath)}" aria-describedby="workspace-help" ${hasNativeFolderPicker ? 'readonly' : ''} />
          <button type="button" class="button" id="choose-workspace">Browse...</button>
        </div>
      </label>
      <p id="workspace-help">Default access: all the folders/files in workspace folder <strong>${escapeHtml(state.workspacePath)}</strong>.</p>
      ${renderCompactDisclosure(
        'workspace-storage',
        'Storage details',
        '<p>Config, data, logs, cache, and local memory stay in this workspace folder for now.</p>',
      )}
    </div>
  `;
}

function renderRuntimeChoices() {
  const mode = runtimeModes.find((item) => item.id === state.runtimeMode) || runtimeModes[0];
  return `
    <div class="interface-grid">
      ${runtimeModes
        .map(
          (mode) => `
            <button type="button" class="interface-card ${state.runtimeMode === mode.id ? 'active' : ''}" data-runtime-mode="${mode.id}">
              <span>${escapeHtml(mode.label)}</span>
              <strong>${escapeHtml(mode.headline)}</strong>
              <p>${escapeHtml(mode.detail)}</p>
            </button>
          `,
        )
        .join('')}
    </div>
    ${renderCompactDisclosure('runtime-preview', 'Preview selected mode', renderRuntimeDemo(mode))}
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

function renderContextAccessChoices() {
  return `
    <div class="channel-grid">
      ${contextAccessOptions
        .map(
          (option) => `
            <label class="check-card compact-choice ${state.selectedContextAccess.includes(option.id) ? 'active' : ''}" data-tooltip="${escapeAttribute(option.detail)}" title="${escapeAttribute(option.detail)}">
              <span class="check-card-head">
                <input type="checkbox" data-context-access="${escapeAttribute(option.id)}" ${checked(state.selectedContextAccess, option.id)} />
                <span>
                  <em>${escapeHtml(option.status)}</em>
                  <strong>${escapeHtml(option.label)}</strong>
                </span>
              </span>
            </label>
          `,
        )
        .join('')}
    </div>
    <div class="capability-chip-row">
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
            <span class="capability-chip" tabindex="0" data-tooltip="${escapeAttribute(detail)}" title="${escapeAttribute(detail)}">${escapeHtml(title)}</span>
          `,
        )
        .join('')}
    </div>
    <p class="muted-line onboarding-wiki-note">Need more detail? Open the GitHub wiki from Help after setup.</p>
  `;
}

function renderProviderChoiceStep() {
  const visibleProviders = providerPresets.filter(
    (item) => (item.access || 'testing') === state.providerCatalogTab,
  );

  return `
    ${renderProgressiveProviderFrame(
      'Provider',
      'Choose who runs the model',
      '<p class="muted-line">Pick one provider. Access, model, and test steps come next.</p>',
    )}
    <div class="provider-stage">
      <div class="provider-access-tabs" aria-label="Provider access">
        ${providerCatalogTabs
          .map(
            (tab) => `
              <button type="button" class="runtime-mode-pill ${state.providerCatalogTab === tab.id ? 'active' : ''}" data-provider-catalog-tab="${escapeAttribute(tab.id)}">
                <strong>${escapeHtml(tab.label)}</strong>
                <span>${escapeHtml(tab.detail)}</span>
              </button>
            `,
          )
          .join('')}
      </div>
      <div class="provider-list provider-choice-list">
        ${visibleProviders
          .map(
            (item) => `
              <article class="provider-card compact-provider-card ${state.providerSelectionConfirmed && state.llmProvider === item.id ? 'active' : ''}" data-tooltip="${escapeAttribute(item.detail || item.label)}" title="${escapeAttribute(item.detail || item.label)}">
                <button type="button" class="provider-select-button" data-provider-id="${item.id}">
                  <strong>${escapeHtml(item.label)}</strong>
                  <span>${item.access === 'stable' ? 'Stable route' : 'Testing access'}</span>
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

function renderProviderCredentialStep(provider = currentProvider(providerPresets, state)) {
  if (!state.providerSelectionConfirmed) {
    return renderStepGuard('Choose a provider first', 'Provider access appears after you pick ChatGPT/OpenAI, MiniMax, local, or another provider.', 3);
  }

  const availableAuthMethods = providerAuthMethods.filter((method) =>
    (provider.authMethods || ['api-key']).includes(method.id),
  );

  if (state.providerAuthMethod === 'browser-account') {
    return `
      <div class="provider-stage">
        <div class="provider-stage-header split-header">
          <div>
            <span class="pill">Authorize</span>
            <h3>${escapeHtml(provider.label)} browser account</h3>
          </div>
          <button type="button" class="button" data-provider-auth-method="api-key">Use API key instead</button>
        </div>
        <div class="provider-focus-panel wide compact-panel">
          ${renderAuthMethodPicker(availableAuthMethods)}
          ${renderCodexOAuthPanel()}
        </div>
      </div>
    `;
  }

  return `
      <div class="provider-stage">
        <div class="provider-stage-header provider-step-toolbar">
          ${renderProviderSwitcher(provider, 'Access')}
          <button type="button" class="button" data-onboarding-step="3">Change provider</button>
        </div>
      <div class="provider-focus-panel wide compact-panel">
        ${renderAuthMethodPicker(availableAuthMethods)}
        <div class="form-grid onboarding-access-grid">
          <label class="full-span">
            API key
            <input id="provider-api-key" type="password" value="${escapeAttribute(state.providerApiKey)}" placeholder="${provider.requiresKey ? 'Required for this provider' : 'Optional for local/custom'}" autocomplete="new-password" />
          </label>
          <label class="full-span">
            Endpoint
            <input id="provider-base-url" value="${escapeAttribute(state.providerBaseUrl)}" placeholder="${escapeAttribute(provider.defaultBaseUrl)}" />
          </label>
          ${renderCompactDisclosure(
            'provider-advanced',
            'Advanced provider details',
            `
              <div class="form-grid two">
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
              </div>
              ${renderProviderAuthGuidance(provider)}
            `,
          )}
          <div class="button-row split">
            <span class="pill ${provider.requiresKey ? 'warn' : 'ok'}">${provider.requiresKey ? 'Key required' : 'Key optional'}</span>
            <button type="button" class="button primary" id="continue-provider-model">Continue to model</button>
          </div>
        </div>
      </div>
    </div>
  `;
}

function renderAuthMethodPicker(availableAuthMethods) {
  return `
    <div class="auth-method-grid compact-auth-grid">
      ${availableAuthMethods
        .map(
          (method) => `
            <button type="button" class="interface-card compact-choice auth-choice-card ${state.providerAuthMethod === method.id ? 'active' : ''}" data-provider-auth-method="${escapeAttribute(method.id)}" data-tooltip="${escapeAttribute(method.detail)}" title="${escapeAttribute(method.detail)}">
              <span>${escapeHtml(method.status)}</span>
              <strong>${escapeHtml(method.label)}</strong>
            </button>
          `,
        )
        .join('')}
    </div>
  `;
}

function renderProviderModelStep(provider = currentProvider(providerPresets, state)) {
  if (!state.providerSelectionConfirmed) {
    return renderStepGuard('Choose a provider first', 'Model choices depend on the selected provider and authorization method.', 3);
  }

  const metadata = modelMetadataFor(state.providerModel, modelMetadata);

  return `
    <div class="provider-stage">
      <div class="provider-stage-header provider-step-toolbar">
        ${renderProviderSwitcher(provider, 'Model')}
        <button type="button" class="button" data-onboarding-step="4">Change access</button>
      </div>
      <div class="provider-focus-panel wide compact-panel">
        <div class="form-grid">
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
        </div>
        <div class="model-detail-panel compact-model-detail">
          <div>
            <span>Context window</span>
            <strong>${escapeHtml(metadata.contextWindow)}</strong>
            <p>${escapeHtml(metadata.currentContextLabel)}</p>
          </div>
          <div>
            <span>Capability overview</span>
            <strong>${escapeHtml(metadata.capabilities.join(', '))}</strong>
          </div>
        </div>
        ${renderCompactDisclosure('model-notes', 'Model notes', `<p>${escapeHtml(metadata.detail)}</p>`)}
      </div>
    </div>
  `;
}

function renderProviderSwitcher(provider, label) {
  return `
    <label class="provider-switcher">
      <span>${escapeHtml(label)}</span>
      <select id="onboarding-provider-select" aria-label="Selected provider">
        ${providerPresets
          .map(
            (item) => `
              <option value="${escapeAttribute(item.id)}" ${selected(provider.id, item.id)}>${escapeHtml(item.label)}</option>
            `,
          )
          .join('')}
      </select>
    </label>
  `;
}

function renderBehaviorStep() {
  return `
    <div class="form-grid two profile-setup-grid compact-profile-grid">
      <label>
        Your name
        <input id="onboarding-user-name" value="${escapeAttribute(state.userName)}" placeholder="What Argentum should call you" />
      </label>
      <label>
        Agent name
        <input id="onboarding-agent-name" value="${escapeAttribute(state.agentName)}" placeholder="Argentum" />
      </label>
      <label class="full-span">
        System prompt
        <textarea id="onboarding-system-prompt" placeholder="Describe how Argentum should behave in this workspace.">${escapeHtml(state.systemPrompt)}</textarea>
        <small>Do not place secrets here. Argentum redacts this from model-visible self-reports.</small>
      </label>
    </div>
    <details class="setup-disclosure" data-disclosure-id="runtime" ${state.onboardingOpenDisclosures.includes('runtime') ? 'open' : ''}>
      <summary>Runtime mode</summary>
      ${renderRuntimeChoices()}
    </details>
    <details class="setup-disclosure" data-disclosure-id="context" ${state.onboardingOpenDisclosures.includes('context') ? 'open' : ''}>
      <summary>Context and capabilities</summary>
      ${renderContextAccessChoices()}
    </details>
    <details class="setup-disclosure" data-disclosure-id="permissions" ${state.onboardingOpenDisclosures.includes('permissions') ? 'open' : ''}>
      <summary>Basic permissions</summary>
      ${renderSecurityChoices()}
    </details>
    <details class="setup-disclosure" data-disclosure-id="channels" ${state.onboardingOpenDisclosures.includes('channels') ? 'open' : ''}>
      <summary>Channels</summary>
      ${renderChannelChoices()}
    </details>
  `;
}

function renderStepGuard(title, detail, step) {
  return `
    <div class="plain-warning">
      <strong>${escapeHtml(title)}</strong>
      <p>${escapeHtml(detail)}</p>
      <button type="button" class="button primary" data-onboarding-step="${step}">Go back</button>
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

function renderCompactDisclosure(id, title, body) {
  return `
    <details class="setup-disclosure inline-disclosure" data-disclosure-id="${escapeAttribute(id)}" ${state.onboardingOpenDisclosures.includes(id) ? 'open' : ''}>
      <summary>${escapeHtml(title)}</summary>
      ${body}
    </details>
  `;
}

function renderProviderAuthGuidance(provider) {
  if (state.providerAuthMethod === 'browser-account') {
    return `
      <div class="setup-guidance compact-guidance">
        <span class="pill">Browser authorization</span>
        <p>Start the flow, approve it in your browser, then return here and complete authorization.</p>
      </div>
    `;
  }

  return `
    <div class="setup-guidance compact-guidance">
      <span class="pill">API key</span>
      <p>Paste a ${escapeHtml(provider.label)} key or use the saved key. Secrets stay outside YAML. More setup detail can live in the GitHub wiki.</p>
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
      <div class="split-header">
        <div>
          <span class="pill">${isBrowserAuth ? 'Selected' : 'Optional'}</span>
          <h3>OpenAI/Codex authorization</h3>
        </div>
        <strong class="oauth-status-text">${escapeHtml(oauth.status || 'idle')}</strong>
      </div>
      <div class="oauth-actions">
        <button type="button" class="button" id="start-codex-oauth">Start authorization</button>
        <button type="button" class="button primary" id="complete-codex-oauth">Complete authorization</button>
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
      </div>
      ${renderCompactDisclosure(
        'oauth-notes',
        'Authorization steps',
        `<p>${escapeHtml(oauth.message || 'OpenAI/Codex authorization has not been started.')}</p><p>Approve Argentum in your browser, then return here and complete authorization.</p>`,
      )}
    </div>
  `;
}

function renderChannelChoices() {
  const selected = new Set(state.selectedChannels);
  const hasWebchat = selected.has('webchat');
  const hasTelegram = selected.has('telegram');
  const hasWhatsApp = selected.has('whatsapp');
  const hasChannelSettings = hasWebchat || hasTelegram || hasWhatsApp;

  return `
    <div class="channel-grid">
      ${channelOptions
        .map(
          (channel) => `
            <label class="check-card compact-choice ${state.selectedChannels.includes(channel.id) ? 'active' : ''}" data-tooltip="${escapeAttribute(channel.detail)}" title="${escapeAttribute(channel.detail)}">
              <span class="check-card-head">
                <input type="checkbox" data-channel-id="${channel.id}" ${checked(state.selectedChannels, channel.id)} ${channel.locked ? 'disabled' : ''} />
                <span>
                  <em>${escapeHtml(channel.status)}</em>
                  <strong>${escapeHtml(channel.label)}</strong>
                </span>
              </span>
            </label>
          `,
        )
        .join('')}
    </div>
    ${
      hasChannelSettings
        ? `
          ${renderCompactDisclosure(
            'channel-settings',
            'Selected channel settings',
            `
              <div class="form-grid two channel-settings">
                ${
                  hasWebchat
                    ? `
                      <label>
                        Webchat token
                        <input id="webchat-token" type="password" value="${escapeAttribute(state.webchatToken)}" placeholder="Generated if left empty" autocomplete="new-password" />
                      </label>
                    `
                    : ''
                }
                ${
                  hasTelegram
                    ? `
                      <label>
                        Telegram bot token
                        <input id="telegram-token" type="password" value="${escapeAttribute(state.telegramToken)}" placeholder="Only needed if Telegram is selected" autocomplete="new-password" />
                      </label>
                      <label>
                        Telegram users/chats
                        <input id="telegram-allowlist" value="${escapeAttribute(state.telegramAllowlist)}" placeholder="Comma-separated user or chat IDs" />
                      </label>
                    `
                    : ''
                }
                ${
                  hasWhatsApp
                    ? `
                      <label>
                        WhatsApp phone ID
                        <input id="whatsapp-phone-id" value="${escapeAttribute(state.whatsappPhoneId)}" placeholder="Advanced: Business API setup" />
                      </label>
                    `
                    : ''
                }
              </div>
            `,
          )}
        `
        : '<p class="muted-line onboarding-wiki-note">Local app is always enabled. Select Telegram, Webchat, or WhatsApp to show their settings.</p>'
    }
  `;
}

function renderSecurityChoices() {
  return `
    <div class="security-choice-grid">
      ${securityProfiles
        .map(
          (profile) => `
            <button type="button" class="interface-card compact-permission-card ${state.securityProfile === profile.id ? 'active' : ''}" data-security-profile="${profile.id}" data-tooltip="${escapeAttribute(profile.detail)}">
              <span>${escapeHtml(profile.label)}</span>
              <strong>${escapeHtml(profile.id === 'restricted' ? 'Best first choice' : profile.label)}</strong>
            </button>
          `,
        )
        .join('')}
    </div>
  `;
}

function renderTestAndStartStep() {
  return `
    <div class="review-pipeline" aria-label="Review test pass">
      <div><span>Review</span><strong>Choices</strong></div>
      <div><span>Test</span><strong>${escapeHtml(providerTestLabel())}</strong></div>
      <div><span>Pass</span><strong>Open Chat</strong></div>
    </div>
    ${renderReviewRows()}
    <div class="button-row split">
      <button type="button" class="button" data-onboarding-step="3">Edit provider</button>
      <button type="button" class="button" data-onboarding-step="6">Edit behavior</button>
      <button type="button" class="button primary" id="test-provider">Test API</button>
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
