import { experienceLevels, onboardingSteps, providerPresets } from './constants.js';
import {
  ensureProviderModelAllowed,
  recordUiEvent,
  setProvider,
  setProviderCatalogTab,
  state,
} from './state.js';
import {
  currentProvider,
  defaultModelForAuth,
  isProbablyAbsolutePath,
  modelOptionsFor,
} from './utils.js';

const ONBOARDING_PROGRESS_STORAGE_KEY = 'argentum.onboardingProgress.v1';

function isDebugEnabled() {
  if (typeof window === 'undefined') return false;
  return (
    window.location.hostname === 'localhost' || window.location.search.includes('debugOnboarding=1')
  );
}

function clampStep(step) {
  const value = Number(step);
  if (!Number.isFinite(value)) return 1;
  return Math.max(1, Math.min(onboardingSteps.length, Math.trunc(value)));
}

function persistableProgress() {
  return {
    onboardingStep: state.onboardingStep,
    experienceLevel: state.experienceLevel,
    runtimeMode: state.runtimeMode,
    llmProvider: state.llmProvider,
    providerApi: state.providerApi,
    providerBaseUrl: state.providerBaseUrl,
    // Deliberately excluded: providerApiKey is held only in memory and not persisted.
    // Storing it in sessionStorage would expose the key in cleartext (CWE-312).
    providerModel: state.providerModel,
    providerAuthMethod: state.providerAuthMethod,
    providerCatalogTab: state.providerCatalogTab,
    providerSetupStage: state.providerSetupStage,
    providerSelectionConfirmed: state.providerSelectionConfirmed,
    customProviderName: state.customProviderName,
    // Deliberately excluded: customApiKeyEnv stores sensitive credential info.
    // Persisting it to sessionStorage would be cleartext storage (CWE-312).
    selectedChannels: state.selectedChannels,
    selectedContextAccess: state.selectedContextAccess,
    securityProfile: state.securityProfile,
    userName: state.userName,
    agentName: state.agentName,
    thinkingLevel: state.thinkingLevel,
    showThinkingInChat: state.showThinkingInChat,
    showThinkingInTelegram: state.showThinkingInTelegram,
  };
}

export function persistOnboardingProgress() {
  if (typeof window === 'undefined') return;
  // Only persist while setup is in progress; once complete, onboarding state is no longer needed
  if (state.setupComplete) return;
  try {
    window.sessionStorage?.setItem(
      ONBOARDING_PROGRESS_STORAGE_KEY,
      JSON.stringify({
        version: state.version,
        savedAt: new Date().toISOString(),
        progress: persistableProgress(),
      }),
    );
  } catch (_error) {
    // Onboarding still works without optional session progress persistence.
  }
}

export function clearOnboardingProgress() {
  if (typeof window === 'undefined') return;
  try {
    window.sessionStorage?.removeItem(ONBOARDING_PROGRESS_STORAGE_KEY);
  } catch (_error) {
    // Optional session progress cleanup should not block the app.
  }
}

export function hydrateOnboardingProgress() {
  if (typeof window === 'undefined' || state.setupComplete) return false;

  try {
    const raw = window.sessionStorage?.getItem(ONBOARDING_PROGRESS_STORAGE_KEY);
    if (!raw) return false;
    const parsed = JSON.parse(raw);
    const progress = parsed?.progress;
    if (!progress || typeof progress !== 'object') return false;

    Object.assign(state, {
      ...progress,
      onboardingStep: clampStep(progress.onboardingStep),
      onboardingOpen: true,
      setupComplete: false,
      onboardingError: '',
      onboardingValidationErrors: [],
      onboardingProgressLoaded: true,
    });
    ensureProviderModelAllowed();
    recordUiEvent('onboarding.progress_restored', 'ok', 'Restored local onboarding progress.', {
      onboardingStep: state.onboardingStep,
      provider: state.providerSelectionConfirmed ? state.llmProvider : 'none',
    });
    return true;
  } catch (error) {
    state.onboardingProgressLoaded = false;
    recordUiEvent('onboarding.progress_restore_failed', 'error', String(error), {});
    return false;
  }
}

function setOnboardingDebug(action, beforeStep, result) {
  state.onboardingDebug = {
    action,
    beforeStep,
    afterStep: state.onboardingStep,
    selectedProvider: state.providerSelectionConfirmed ? state.llmProvider : '',
    selectedModel: state.providerModel || '',
    validationResult: result.valid ? 'valid' : 'blocked',
    validationErrors: result.errors || [],
    nextStepTarget: result.targetStep || null,
    navigationAttempted: Boolean(result.navigationAttempted),
    persistedOnboardingStatus: state.setupComplete ? 'complete' : 'in-progress',
    timestamp: new Date().toISOString(),
  };

  if (isDebugEnabled()) {
    console.debug('[Argentum onboarding]', state.onboardingDebug);
  }
}

function blockNavigation(action, beforeStep, errors, targetStep = null) {
  const result = {
    ok: false,
    valid: false,
    errors,
    message: errors[0] || 'Onboarding cannot continue yet.',
    targetStep,
    navigationAttempted: true,
  };
  state.onboardingError = result.message;
  state.onboardingValidationErrors = errors;
  setOnboardingDebug(action, beforeStep, result);
  recordUiEvent('onboarding.validation_blocked', 'blocked', result.message, {
    action,
    onboardingStep: beforeStep,
    targetStep,
    errors,
  });
  return result;
}

function allowNavigation(action, beforeStep, targetStep, extra = {}) {
  const result = {
    ok: true,
    valid: true,
    errors: [],
    message: '',
    targetStep,
    navigationAttempted: true,
    ...extra,
  };
  state.onboardingError = '';
  state.onboardingValidationErrors = [];
  setOnboardingDebug(action, beforeStep, result);
  persistOnboardingProgress();
  return result;
}

export function clearOnboardingError() {
  state.onboardingError = '';
  state.onboardingValidationErrors = [];
}

export function validateCurrentStep(step = state.onboardingStep) {
  const currentStep = clampStep(step);
  const errors = [];

  if (currentStep === 1 && !state.experienceLevel.trim()) {
    errors.push('Choose Beginner, Intermediate, or Expert before continuing.');
  }

  if (currentStep === 2 || currentStep === onboardingSteps.length) {
    if (!state.workspacePath.trim()) {
      errors.push('Choose a workspace folder before continuing.');
    } else if (!isProbablyAbsolutePath(state.workspacePath)) {
      errors.push('Workspace path must be a full folder path.');
    }
  }

  if (
    (currentStep === 3 ||
      currentStep === 4 ||
      currentStep === 5 ||
      currentStep === onboardingSteps.length) &&
    !state.providerSelectionConfirmed
  ) {
    errors.push('Choose an AI provider before continuing.');
  }

  if (currentStep === 4 || currentStep === onboardingSteps.length) {
    const provider = currentProvider(providerPresets, state);
    if (!state.providerBaseUrl.trim()) {
      errors.push('Add the provider endpoint before continuing.');
    }

    if (
      state.providerAuthMethod === 'api-key' &&
      provider.requiresKey &&
      !state.providerApiKey.trim() &&
      !state.setupComplete
    ) {
      errors.push(
        'Add an API key, choose a local provider, or use browser account authorization before continuing.',
      );
    }

    if (
      state.providerAuthMethod === 'browser-account' &&
      state.codexOAuth.status !== 'ok' &&
      !state.setupComplete
    ) {
      errors.push(
        'Complete OpenAI/Codex authorization before continuing with browser account auth.',
      );
    }
  }

  if (
    (currentStep === 5 || currentStep === onboardingSteps.length) &&
    !state.providerModel.trim()
  ) {
    errors.push('Choose a model before continuing.');
  }

  return {
    valid: errors.length === 0,
    errors,
    message: errors[0] || '',
    step: currentStep,
  };
}

export function selectExperienceLevel(levelId) {
  const beforeStep = state.onboardingStep;
  const level = experienceLevels.find((item) => item.id === levelId);
  if (!level) {
    return blockNavigation('selectExperienceLevel', beforeStep, [
      'Choose Beginner, Intermediate, or Expert before continuing.',
    ]);
  }

  state.experienceLevel = level.id;
  clearOnboardingError();
  persistOnboardingProgress();
  recordUiEvent('onboarding.experience_selected', 'ok', `Experience set to ${level.label}.`, {
    experienceLevel: level.id,
  });
  return allowNavigation('selectExperienceLevel', beforeStep, beforeStep, {
    navigationAttempted: false,
  });
}

export function selectProvider(providerId) {
  const beforeStep = state.onboardingStep;
  const provider = providerPresets.find((item) => item.id === providerId);
  if (!provider) {
    return blockNavigation('selectProvider', beforeStep, [
      'Choose a valid AI provider before continuing.',
    ]);
  }

  setProvider(provider);
  setProviderCatalogTab(provider.access || 'testing');
  state.providerSelectionConfirmed = true;
  state.providerSetupStage = 'auth';
  clearOnboardingError();
  persistOnboardingProgress();
  recordUiEvent('onboarding.provider_selected', 'ok', `Provider set to ${provider.label}.`, {
    provider: provider.id,
    authMethod: state.providerAuthMethod,
    model: state.providerModel,
  });
  return allowNavigation('selectProvider', beforeStep, beforeStep, { navigationAttempted: false });
}

export function selectAuthMethod(authMethod) {
  const beforeStep = state.onboardingStep;
  const provider = currentProvider(providerPresets, state);
  const allowedAuthMethods = provider.authMethods || ['api-key'];
  if (!allowedAuthMethods.includes(authMethod)) {
    return blockNavigation('selectAuthMethod', beforeStep, [
      `${provider.label} does not support that authorization method.`,
    ]);
  }

  state.providerAuthMethod = authMethod;
  state.providerModel = defaultModelForAuth(provider, authMethod);
  state.providerSetupStage = 'credentials';
  state.apiTest = {
    status: authMethod === 'browser-account' ? 'warning' : 'idle',
    message:
      authMethod === 'browser-account'
        ? 'OpenAI/Codex authorization selected. Start and complete authorization before testing provider access.'
        : 'API key authorization selected. Add the key, then choose a model.',
  };
  clearOnboardingError();
  persistOnboardingProgress();
  recordUiEvent(
    'onboarding.auth_method_selected',
    'ok',
    `Authorization method set to ${authMethod}.`,
    {
      provider: provider.id,
      authMethod,
    },
  );
  return allowNavigation('selectAuthMethod', beforeStep, beforeStep, {
    navigationAttempted: false,
  });
}

export function setProviderSetupStage(stage) {
  const beforeStep = state.onboardingStep;
  const nextStage = stage || 'provider';
  const allowedStages = ['provider', 'auth', 'credentials', 'model'];
  if (!allowedStages.includes(nextStage)) {
    return blockNavigation('setProviderSetupStage', beforeStep, [
      'Choose a valid provider setup step.',
    ]);
  }

  if (nextStage === 'provider') {
    state.providerSetupStage = 'provider';
    state.providerSelectionConfirmed = false;
    clearOnboardingError();
    persistOnboardingProgress();
    recordUiEvent('onboarding.provider_stage_changed', 'ok', 'Provider selection reopened.', {
      providerSetupStage: state.providerSetupStage,
    });
    return allowNavigation('setProviderSetupStage', beforeStep, beforeStep, {
      navigationAttempted: false,
    });
  }

  if (!state.providerSelectionConfirmed) {
    return blockNavigation('setProviderSetupStage', beforeStep, [
      'Choose an AI provider before continuing.',
    ]);
  }

  if (nextStage === 'model') {
    const provider = currentProvider(providerPresets, state);
    if (
      state.providerAuthMethod === 'api-key' &&
      provider.requiresKey &&
      !state.providerApiKey.trim() &&
      !state.setupComplete
    ) {
      return blockNavigation('setProviderSetupStage', beforeStep, [
        'Add the API key first, or go back and choose browser account authorization.',
      ]);
    }

    if (
      state.providerAuthMethod === 'browser-account' &&
      state.codexOAuth.status !== 'ok' &&
      !state.setupComplete
    ) {
      return blockNavigation('setProviderSetupStage', beforeStep, [
        'Complete OpenAI/Codex authorization before choosing a model.',
      ]);
    }

    ensureProviderModelAllowed();
  }

  state.providerSetupStage = nextStage;
  clearOnboardingError();
  persistOnboardingProgress();
  recordUiEvent(
    'onboarding.provider_stage_changed',
    'ok',
    `Provider setup moved to ${nextStage}.`,
    {
      providerSetupStage: state.providerSetupStage,
    },
  );
  return allowNavigation('setProviderSetupStage', beforeStep, beforeStep, {
    navigationAttempted: false,
  });
}

export function selectModel(modelId) {
  const beforeStep = state.onboardingStep;
  const provider = currentProvider(providerPresets, state);
  const model = modelOptionsFor(provider, state.providerModel, state.providerAuthMethod).find(
    (item) => item.id === modelId,
  );
  if (!model) {
    return blockNavigation('selectModel', beforeStep, [
      `Choose a supported ${provider.label} model before continuing.`,
    ]);
  }

  state.providerModel = model.id;
  state.apiTest = {
    status: 'idle',
    message: 'Model changed. Test the provider before using live chat.',
  };
  clearOnboardingError();
  persistOnboardingProgress();
  recordUiEvent('onboarding.model_selected', 'ok', `Model set to ${model.id}.`, {
    provider: provider.id,
    model: model.id,
  });
  return allowNavigation('selectModel', beforeStep, beforeStep, { navigationAttempted: false });
}

export function goToStep(targetStep) {
  const beforeStep = state.onboardingStep;
  const nextStep = clampStep(targetStep);
  if (nextStep > beforeStep + 1) {
    return blockNavigation(
      'goToStep',
      beforeStep,
      ['Finish the current setup step before jumping ahead.'],
      nextStep,
    );
  }

  if (nextStep > beforeStep) {
    const validation = validateCurrentStep(beforeStep);
    if (!validation.valid) {
      return blockNavigation('goToStep', beforeStep, validation.errors, nextStep);
    }
  }

  if (beforeStep === 4 && nextStep >= 5) {
    state.providerSetupStage = 'model';
    ensureProviderModelAllowed();
  }

  state.onboardingStep = nextStep;
  recordUiEvent('onboarding.step_selected', 'ok', `Onboarding moved to step ${nextStep}.`, {
    onboardingStep: nextStep,
  });
  return allowNavigation('goToStep', beforeStep, nextStep);
}

export function nextStep() {
  const beforeStep = state.onboardingStep;
  const validation = validateCurrentStep(beforeStep);
  const targetStep = Math.min(onboardingSteps.length, beforeStep + 1);

  if (!validation.valid) {
    return blockNavigation('nextStep', beforeStep, validation.errors, targetStep);
  }

  if (beforeStep === 4 && targetStep >= 5) {
    state.providerSetupStage = 'model';
    ensureProviderModelAllowed();
  }

  state.onboardingStep = targetStep;
  recordUiEvent('onboarding.step_next', 'ok', `Onboarding moved to step ${targetStep}.`, {
    onboardingStep: targetStep,
  });
  return allowNavigation('nextStep', beforeStep, targetStep);
}

export function previousStep() {
  const beforeStep = state.onboardingStep;
  const targetStep = Math.max(1, beforeStep - 1);
  state.onboardingStep = targetStep;
  recordUiEvent('onboarding.step_back', 'ok', `Onboarding moved back to step ${targetStep}.`, {
    onboardingStep: targetStep,
  });
  return allowNavigation('previousStep', beforeStep, targetStep);
}

export function completeOnboarding(saveResult = {}) {
  const beforeStep = state.onboardingStep;
  const validation = validateCurrentStep(beforeStep);
  if (!validation.valid) {
    return blockNavigation(
      'completeOnboarding',
      beforeStep,
      validation.errors,
      onboardingSteps.length,
    );
  }

  state.setupComplete = true;
  state.setupStatus = 'setup_saved';
  state.savedConfigPath =
    saveResult.configPath || saveResult.config_path || state.savedConfigPath || '';
  state.providerApiKey = '';
  state.webchatToken = '';
  state.telegramToken = '';
  state.activeSection = 'chat';
  state.onboardingOpen = false;
  state.onboardingStep = onboardingSteps.length;
  clearOnboardingError();
  clearOnboardingProgress();
  recordUiEvent('onboarding.completed', 'ok', 'Onboarding completed and Chat opened.', {
    savedConfigPath: state.savedConfigPath,
  });
  const result = allowNavigation('completeOnboarding', beforeStep, onboardingSteps.length);
  clearOnboardingProgress();
  return result;
}

export function restartOnboardingState() {
  const beforeStep = state.onboardingStep;
  state.onboardingOpen = true;
  state.onboardingStep = 1;
  state.setupStatus = state.setupComplete ? 'setup_reviewing' : 'setup_pending';
  clearOnboardingError();
  persistOnboardingProgress();
  recordUiEvent('onboarding.restarted', 'ok', 'Onboarding was opened.', {
    setupComplete: state.setupComplete,
  });
  return allowNavigation('restartOnboardingState', beforeStep, 1);
}
