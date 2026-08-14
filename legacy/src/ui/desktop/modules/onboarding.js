import {
  channelOptions,
  contextAccessOptions,
  experienceLevels,
  llamaDownloadPresets,
  modelMetadata,
  onboardingSteps,
  providerAuthMethods,
  providerCatalogTabs,
  providerPresets,
  runtimeModes,
  securityProfiles,
} from './constants.js';
import { validateCurrentStep } from './onboarding-controller.js';
import { state } from './state.js';
import {
  checked,
  currentProvider,
  displayModelName,
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
  label: 'Setup',
  render: renderOnboarding,
  validate: () => validateCurrentStep().message,
  healthCheck: () => ({
    status: state.setupComplete ? 'ok' : 'pending',
    message: state.setupComplete ? 'Setup complete' : 'Setup is in progress',
  }),
};

const stepDescriptions = [
  'Choose how much guidance you want.',
  'Select the folder Argentum is allowed to use.',
  'Choose where your model runs.',
  'Connect Argentum to that provider.',
  'Pick the model for this workspace.',
  'Set behavior and permission defaults.',
  'Review the setup and test the connection.',
];

function renderOnboarding() {
  const stepIndex = state.onboardingStep - 1;
  const finalStep = state.onboardingStep === onboardingSteps.length;

  return `
    <div class="setup-overlay">
      <section
        class="setup-window"
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
      >
        <aside class="setup-sidebar">
          <div class="setup-brand">
            <span class="argentum-mark" aria-hidden="true">A</span>
            <div>
              <strong>Argentum</strong>
              <span>Workspace setup</span>
            </div>
          </div>

          <nav class="setup-progress" aria-label="Setup progress">
            ${onboardingSteps
              .map((step, index) => {
                const number = index + 1;
                const current = number === state.onboardingStep;
                const complete = number < state.onboardingStep;
                return `
                  <button
                    type="button"
                    class="${current ? 'is-current' : ''} ${complete ? 'is-complete' : ''}"
                    data-onboarding-step="${number}"
                    aria-current="${current ? 'step' : 'false'}"
                  >
                    <span>${complete ? '<span data-icon="check"></span>' : number}</span>
                    <strong>${escapeHtml(step)}</strong>
                  </button>
                `;
              })
              .join('')}
          </nav>

          <div class="setup-sidebar-note">
            <strong>Private by default</strong>
            <p>Argentum does not collect analytics or send workspace data anywhere on its own.</p>
          </div>
        </aside>

        <div class="setup-content">
          <header class="setup-header">
            <div>
              <p class="section-kicker">Step ${state.onboardingStep} of ${onboardingSteps.length}</p>
              <h2 id="onboarding-title">${escapeHtml(onboardingSteps[stepIndex])}</h2>
              <p>${escapeHtml(stepDescriptions[stepIndex])}</p>
            </div>
            <button
              class="icon-button"
              type="button"
              data-cancel-onboarding="true"
              title="Exit setup"
              aria-label="Exit setup"
            >
              <span data-icon="x"></span>
            </button>
          </header>

          <div class="setup-scroll-region">
            ${renderValidationError()}
            ${renderStep()}
          </div>

          <footer class="setup-footer">
            <button class="button quiet" type="button" data-cancel-onboarding="true">
              ${state.setupComplete ? 'Close setup' : 'Finish later'}
            </button>
            <div class="button-row">
              <button
                class="button"
                type="button"
                id="back-button"
                ${state.onboardingStep === 1 ? 'disabled' : ''}
              >Back</button>
              <button class="button primary" type="button" id="next-button">
                ${finalStep ? 'Save and open Argentum' : 'Continue'}
              </button>
            </div>
          </footer>
        </div>
      </section>
    </div>
  `;
}

function renderValidationError() {
  if (!state.onboardingError) return '';
  return `
    <div class="setup-error" role="alert">
      <span data-icon="alert" aria-hidden="true"></span>
      <div>
        <strong>Setup needs attention</strong>
        <p>${escapeHtml(state.onboardingError)}</p>
      </div>
    </div>
  `;
}

function renderStep() {
  const renderers = [
    renderWelcomeStep,
    renderWorkspaceStep,
    renderProviderStep,
    renderAccessStep,
    renderModelStep,
    renderPreferencesStep,
    renderReviewStep,
  ];
  return renderers[state.onboardingStep - 1]?.() || '';
}

function renderWelcomeStep() {
  return `
    <div class="setup-section-intro">
      <p class="section-kicker">Start simple</p>
      <h3>How familiar are you with local AI tools?</h3>
      <p>This changes how much explanation Argentum shows. It does not remove any features.</p>
    </div>

    <div class="choice-grid three">
      ${experienceLevels
        .map(
          (level) => `
            <button
              class="choice-card ${state.experienceLevel === level.id ? 'is-selected' : ''}"
              type="button"
              data-experience-level="${escapeAttribute(level.id)}"
              aria-pressed="${state.experienceLevel === level.id ? 'true' : 'false'}"
            >
              <span class="choice-check" aria-hidden="true"></span>
              <strong>${escapeHtml(level.label)}</strong>
              <h4>${escapeHtml(level.headline)}</h4>
              <p>${escapeHtml(level.detail)}</p>
            </button>
          `,
        )
        .join('')}
    </div>

    <aside class="setup-note">
      <strong>You remain in control</strong>
      <p>Shell commands, external folders, browser data, and network actions still require the permission profile you choose later.</p>
    </aside>
  `;
}

function renderWorkspaceStep() {
  const nativePicker = typeof window !== 'undefined' && Boolean(window.__TAURI__?.dialog?.open);
  const explainedPath = explainPath(state.workspacePath);

  return `
    <div class="setup-section-intro">
      <p class="section-kicker">Workspace boundary</p>
      <h3>Choose one folder</h3>
      <p>Argentum stores configuration, local data, logs, models, and chat history inside this workspace.</p>
    </div>

    <section class="workspace-choice">
      <div class="workspace-choice-icon" data-icon="folder" aria-hidden="true"></div>
      <div>
        <label for="workspace-input">Workspace folder</label>
        <div class="input-action-row">
          <input
            id="workspace-input"
            value="${escapeAttribute(state.workspacePath)}"
            placeholder="Choose a folder"
            ${nativePicker ? 'readonly' : ''}
          />
          <button class="button primary" id="choose-workspace" type="button">Browse</button>
        </div>
        <p>${escapeHtml(explainedPath)}</p>
      </div>
    </section>

    <div class="boundary-list">
      <article>
        <span data-icon="check"></span>
        <div>
          <strong>Inside the workspace</strong>
          <p>Read and write access follows your selected permission profile.</p>
        </div>
      </article>
      <article>
        <span data-icon="lock"></span>
        <div>
          <strong>Outside the workspace</strong>
          <p>Argentum must ask before accessing another folder or application.</p>
        </div>
      </article>
      <article>
        <span data-icon="shield"></span>
        <div>
          <strong>Secrets</strong>
          <p>Provider credentials are kept out of the workspace YAML and never shown after entry.</p>
        </div>
      </article>
    </div>

    ${renderMigrationCard()}
  `;
}

function renderMigrationCard() {
  if (state.migrationSkipped || state.migrationResults) return '';
  const source = state.migrationSources?.openclaw?.found
    ? ['OpenClaw', state.migrationSources.openclaw]
    : state.migrationSources?.hermes?.found
      ? ['Hermes', state.migrationSources.hermes]
      : null;
  if (!source) return '';

  return `
    <aside class="migration-card">
      <div>
        <span class="status-tag">Existing setup found</span>
        <h4>Import from ${escapeHtml(source[0])}</h4>
        <p>${(source[1].items || []).length} supported items can be copied into this workspace.</p>
      </div>
      <div class="button-row">
        <button class="button primary" id="do-migration" type="button">Import</button>
        <button class="button quiet" id="skip-migration" type="button">Skip</button>
      </div>
    </aside>
  `;
}

function renderProviderStep() {
  const visibleProviders = providerPresets.filter(
    (provider) => (provider.access || 'testing') === state.providerCatalogTab,
  );

  return `
    <div class="setup-section-intro">
      <p class="section-kicker">Model provider</p>
      <h3>Where should requests run?</h3>
      <p>Choose a local runtime or a hosted provider. Argentum only sends a request when you submit a chat.</p>
    </div>

    <div class="segmented-control provider-tabs" role="tablist" aria-label="Provider groups">
      ${providerCatalogTabs
        .map(
          (tab) => `
            <button
              class="${state.providerCatalogTab === tab.id ? 'is-active' : ''}"
              type="button"
              data-provider-catalog-tab="${escapeAttribute(tab.id)}"
            >
              <strong>${escapeHtml(tab.label)}</strong>
              <span>${escapeHtml(tab.detail)}</span>
            </button>
          `,
        )
        .join('')}
    </div>

    <div class="provider-grid">
      ${visibleProviders
        .map((provider) => {
          const active = state.providerSelectionConfirmed && state.llmProvider === provider.id;
          const local =
            provider.defaultBaseUrl.startsWith('http://127.0.0.1') ||
            provider.defaultBaseUrl.startsWith('http://localhost');
          return `
            <article class="provider-choice ${active ? 'is-selected' : ''}">
              <button
                class="provider-choice-main"
                type="button"
                data-provider-id="${escapeAttribute(provider.id)}"
                aria-pressed="${active ? 'true' : 'false'}"
              >
                <span class="provider-choice-topline">
                  <span class="status-tag ${local ? 'local' : ''}">${local ? 'Local' : 'Hosted'}</span>
                  <span class="choice-check" aria-hidden="true"></span>
                </span>
                <strong>${escapeHtml(provider.label)}</strong>
                <p>${escapeHtml(provider.detail)}</p>
                <span class="provider-route">${escapeHtml(provider.dataRegion)}</span>
              </button>
              <a
                href="${escapeAttribute(provider.websiteUrl)}"
                data-open-external="${escapeAttribute(provider.websiteUrl)}"
              >Provider website <span data-icon="externalLink"></span></a>
            </article>
          `;
        })
        .join('')}
    </div>
  `;
}

function renderAccessStep() {
  if (!state.providerSelectionConfirmed) return renderStepGuard('Choose a provider first', 3);
  const provider = currentProvider(providerPresets, state);
  const authMethods = providerAuthMethods.filter((method) =>
    (provider.authMethods || ['api-key']).includes(method.id),
  );

  return `
    <div class="setup-section-intro split">
      <div>
        <p class="section-kicker">Provider access</p>
        <h3>Connect ${escapeHtml(provider.label)}</h3>
        <p>${escapeHtml(provider.dataRoute)}</p>
      </div>
      <button class="button quiet" type="button" data-onboarding-step="3">Change provider</button>
    </div>

    ${
      authMethods.length > 1
        ? `
          <div class="auth-method-grid">
            ${authMethods
              .map(
                (method) => `
                  <button
                    class="choice-card compact ${state.providerAuthMethod === method.id ? 'is-selected' : ''}"
                    type="button"
                    data-provider-auth-method="${escapeAttribute(method.id)}"
                  >
                    <span class="choice-check"></span>
                    <strong>${escapeHtml(method.label)}</strong>
                    <p>${escapeHtml(method.detail)}</p>
                  </button>
                `,
              )
              .join('')}
          </div>
        `
        : ''
    }

    ${
      state.providerAuthMethod === 'browser-account'
        ? renderBrowserAuthorization()
        : renderApiAccess(provider)
    }
  `;
}

function renderBrowserAuthorization() {
  const authorization = state.codexOAuth || {};
  const ready = authorization.status === 'ok';
  return `
    <section class="credential-panel">
      <div class="credential-panel-header">
        <div>
          <span class="status-tag ${ready ? 'success' : ''}">${ready ? 'Connected' : 'Authorization required'}</span>
          <h4>Use your OpenAI or Codex account</h4>
          <p>Argentum opens the official authorization page. It never asks for your account password.</p>
        </div>
      </div>
      <div class="oauth-actions">
        <button class="button primary" id="start-codex-oauth" type="button">Start authorization</button>
        <button class="button" id="complete-codex-oauth" type="button">I completed authorization</button>
      </div>
      ${
        authorization.userCode
          ? `
            <div class="oauth-code">
              <span>Verification code</span>
              <strong>${escapeHtml(authorization.userCode)}</strong>
            </div>
          `
          : ''
      }
      <p class="field-help">${escapeHtml(authorization.message || 'Authorization has not started.')}</p>
    </section>
  `;
}

function renderApiAccess(provider) {
  return `
    <section class="credential-panel">
      <div class="form-grid">
        <label class="full-span">
          <span>API key ${provider.requiresKey ? '' : '(optional)'}</span>
          <input
            id="provider-api-key"
            type="password"
            value="${escapeAttribute(state.providerApiKey)}"
            placeholder="${provider.requiresKey ? 'Paste the provider key' : 'Leave empty for a local server'}"
            autocomplete="new-password"
          />
          <small id="provider-key-status">
            ${provider.requiresKey ? 'Stored securely outside workspace configuration.' : 'Most local servers do not need a key.'}
          </small>
        </label>

        <label class="full-span">
          <span>Endpoint</span>
          <input
            id="provider-base-url"
            value="${escapeAttribute(state.providerBaseUrl)}"
            placeholder="${escapeAttribute(provider.defaultBaseUrl)}"
          />
        </label>

        <label>
          <span>API format</span>
          <select id="provider-api">
            <option value="openai" ${selected(state.providerApi, 'openai')}>OpenAI compatible</option>
            <option value="anthropic" ${selected(state.providerApi, 'anthropic')}>Anthropic compatible</option>
          </select>
        </label>

        ${
          state.llmProvider === 'custom'
            ? `
              <label>
                <span>Provider name</span>
                <input id="custom-provider-name" value="${escapeAttribute(state.customProviderName)}" />
              </label>
              <label>
                <span>Secret environment variable</span>
                <input id="custom-api-key-env" value="${escapeAttribute(state.customApiKeyEnv)}" />
              </label>
            `
            : ''
        }
      </div>
    </section>
  `;
}

function renderModelStep() {
  if (!state.providerSelectionConfirmed) return renderStepGuard('Choose a provider first', 3);
  const provider = currentProvider(providerPresets, state);
  const options = modelOptionsFor(provider, state.providerModel, state.providerAuthMethod);
  const selectedMetadata = modelMetadataFor(state.providerModel, modelMetadata);

  return `
    <div class="setup-section-intro split">
      <div>
        <p class="section-kicker">Workspace model</p>
        <h3>Choose a model</h3>
        <p>Start with a smaller model for speed. Move up when a task needs deeper reasoning, coding, or image understanding.</p>
      </div>
      <span class="provider-badge">${escapeHtml(provider.label)}</span>
    </div>

    <div class="model-choice-grid">
      ${options
        .map((option) => {
          const metadata = modelMetadataFor(option.id, modelMetadata);
          const preset = llamaDownloadPresets.find((candidate) => candidate.modelId === option.id);
          const active = option.id === state.providerModel;
          const detail = preset?.detail || metadata.detail;
          const capabilities = metadata.capabilities || (preset?.role ? [preset.role] : ['chat']);
          return `
            <button
              class="model-choice ${active ? 'is-selected' : ''}"
              type="button"
              data-provider-model="${escapeAttribute(option.id)}"
              aria-pressed="${active ? 'true' : 'false'}"
            >
              <span class="model-choice-topline">
                <strong>${escapeHtml(option.label)}</strong>
                <span class="choice-check"></span>
              </span>
              <p>${escapeHtml(detail)}</p>
              <span class="capability-list">
                ${capabilities
                  .slice(0, 3)
                  .map((capability) => `<em>${escapeHtml(capability)}</em>`)
                  .join('')}
              </span>
              ${preset ? `<small>${escapeHtml(`${preset.size} · ${preset.license}`)}</small>` : ''}
            </button>
          `;
        })
        .join('')}
    </div>

    ${
      ['custom-model', 'custom-local-model'].includes(state.providerModel)
        ? `
          <label class="custom-model-field">
            <span>Exact model ID</span>
            <input id="provider-custom-model" value="" placeholder="provider/model-name" />
          </label>
        `
        : ''
    }

    <aside class="selected-model-summary">
      <div>
        <span class="status-tag success">Selected</span>
        <h4>${escapeHtml(displayModelName(state.providerModel))}</h4>
        <p>${escapeHtml(selectedMetadata.detail)}</p>
      </div>
      ${
        state.llmProvider === 'llama-cpp'
          ? `
            <div class="button-row">
              <button class="button" id="choose-llama-model" type="button">Choose local GGUF</button>
              <button class="button quiet" type="button" data-section="local-server">Open local runtime</button>
            </div>
          `
          : ''
      }
    </aside>
  `;
}

function renderPreferencesStep() {
  return `
    <div class="setup-section-intro">
      <p class="section-kicker">Defaults</p>
      <h3>How should Argentum work?</h3>
      <p>These settings can all be changed later.</p>
    </div>

    <div class="preference-layout">
      <section class="preference-section">
        <h4>Identity</h4>
        <div class="form-grid two">
          <label>
            <span>Your name</span>
            <input id="onboarding-user-name" value="${escapeAttribute(state.userName)}" placeholder="Optional" />
          </label>
          <label>
            <span>Assistant name</span>
            <input id="onboarding-agent-name" value="${escapeAttribute(state.agentName)}" />
          </label>
          <label class="full-span">
            <span>System instructions</span>
            <textarea id="onboarding-system-prompt" rows="4">${escapeHtml(state.systemPrompt)}</textarea>
          </label>
        </div>
      </section>

      <section class="preference-section">
        <h4>Primary workflow</h4>
        <div class="choice-grid two">
          ${runtimeModes
            .map(
              (mode) => `
                <button
                  class="choice-card compact ${state.runtimeMode === mode.id ? 'is-selected' : ''}"
                  type="button"
                  data-runtime-mode="${escapeAttribute(mode.id)}"
                >
                  <span class="choice-check"></span>
                  <strong>${escapeHtml(mode.label)}</strong>
                  <p>${escapeHtml(mode.detail)}</p>
                </button>
              `,
            )
            .join('')}
        </div>
      </section>

      <section class="preference-section">
        <h4>Permission profile</h4>
        <div class="choice-grid two">
          ${securityProfiles
            .map(
              (profile) => `
                <button
                  class="choice-card compact ${state.securityProfile === profile.id ? 'is-selected' : ''}"
                  type="button"
                  data-security-profile="${escapeAttribute(profile.id)}"
                >
                  <span class="choice-check"></span>
                  <strong>${escapeHtml(profile.label)}</strong>
                  <p>${escapeHtml(profile.detail)}</p>
                </button>
              `,
            )
            .join('')}
        </div>
      </section>

      <section class="preference-section">
        <h4>Optional context</h4>
        <div class="check-list">
          ${contextAccessOptions
            .map(
              (option) => `
                <label class="${state.selectedContextAccess.includes(option.id) ? 'is-selected' : ''}">
                  <input
                    type="checkbox"
                    data-context-access="${escapeAttribute(option.id)}"
                    ${checked(state.selectedContextAccess, option.id)}
                  />
                  <span>
                    <strong>${escapeHtml(option.label)}</strong>
                    <small>${escapeHtml(option.detail)}</small>
                  </span>
                </label>
              `,
            )
            .join('')}
        </div>
      </section>

      <section class="preference-section">
        <h4>Channels</h4>
        <div class="check-list">
          ${channelOptions
            .map(
              (channel) => `
                <label class="${state.selectedChannels.includes(channel.id) ? 'is-selected' : ''}">
                  <input
                    type="checkbox"
                    data-channel-id="${escapeAttribute(channel.id)}"
                    ${checked(state.selectedChannels, channel.id)}
                    ${channel.locked ? 'disabled' : ''}
                  />
                  <span>
                    <strong>${escapeHtml(channel.label)}</strong>
                    <small>${escapeHtml(channel.detail)}</small>
                  </span>
                </label>
              `,
            )
            .join('')}
        </div>
      </section>
    </div>
  `;
}

function renderReviewStep() {
  const provider = currentProvider(providerPresets, state);
  const testReady = state.apiTest.status === 'ok';

  const rows = [
    ['Workspace', state.workspacePath || 'Not selected'],
    ['Provider', provider.label],
    ['Model', displayModelName(state.providerModel)],
    ['Authorization', labelFor(providerAuthMethods, state.providerAuthMethod)],
    ['Workflow', labelFor(runtimeModes, state.runtimeMode)],
    ['Permissions', labelFor(securityProfiles, state.securityProfile)],
  ];

  return `
    <div class="setup-section-intro">
      <p class="section-kicker">Final check</p>
      <h3>Ready to open Argentum</h3>
      <p>Review the workspace boundary and connection before saving.</p>
    </div>

    <div class="review-layout">
      <dl class="review-list">
        ${rows
          .map(
            ([label, value]) => `
              <div>
                <dt>${escapeHtml(label)}</dt>
                <dd>${escapeHtml(value)}</dd>
              </div>
            `,
          )
          .join('')}
      </dl>

      <section class="connection-test ${testReady ? 'is-ready' : ''}">
        <span class="connection-test-icon" data-icon="${testReady ? 'check' : 'pulse'}"></span>
        <div>
          <span class="status-tag ${testReady ? 'success' : ''}">
            ${testReady ? 'Connection ready' : 'Connection not tested'}
          </span>
          <h4>${escapeHtml(provider.label)}</h4>
          <p>${escapeHtml(state.apiTest.message)}</p>
        </div>
        <button
          class="button ${testReady ? '' : 'primary'}"
          id="test-provider"
          type="button"
          ${state.apiTest.status === 'testing' ? 'disabled' : ''}
        >${state.apiTest.status === 'testing' ? 'Testing...' : 'Test connection'}</button>
      </section>
    </div>

    <aside class="setup-note">
      <strong>No hidden telemetry</strong>
      <p>Argentum does not run analytics or maintain an external account database. Provider requests follow the provider and endpoint selected above.</p>
    </aside>
  `;
}

function renderStepGuard(title, step) {
  return `
    <div class="step-guard">
      <span data-icon="alert"></span>
      <h3>${escapeHtml(title)}</h3>
      <p>Go back and finish the required setup choice.</p>
      <button class="button primary" type="button" data-onboarding-step="${step}">Go to step ${step}</button>
    </div>
  `;
}
