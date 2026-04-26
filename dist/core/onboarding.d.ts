export type ProviderName = 'minimax' | 'groq' | 'ollama' | 'nvidia' | 'openrouter' | 'google' | 'anthropic' | 'openai' | 'custom';
export interface ProviderPreset {
    name: string;
    label: string;
    base_url: string;
    api_key_env: string;
    api: 'openai' | 'anthropic';
    defaultModel: string;
    headers?: Record<string, string>;
}
export interface OnboardingTelegramOptions {
    token?: string;
    allowedUsers?: number[];
    allowedChats?: number[];
    allowAll?: boolean;
}
export interface OnboardingOptions {
    name?: string;
    provider?: ProviderName;
    customProvider?: Partial<ProviderPreset>;
    model?: string;
    apiKey?: string;
    port?: number;
    featureCategories?: string[];
    webchatAuthToken?: string;
    telegram?: OnboardingTelegramOptions;
}
export interface OnboardingProfile {
    config: Record<string, unknown>;
    env: Record<string, string>;
    warnings: string[];
}
export interface WrittenOnboardingProfile {
    configPath: string;
    envPath: string;
    envExamplePath: string;
    dataDir: string;
}
export declare const PROVIDER_PRESETS: Record<Exclude<ProviderName, 'custom'>, ProviderPreset>;
export declare function generateWebchatAuthToken(): string;
export declare function createOnboardingProfile(options?: OnboardingOptions): OnboardingProfile;
export declare function writeOnboardingProfile(workDir: string, profile: OnboardingProfile, options?: {
    overwrite?: boolean;
}): WrittenOnboardingProfile;
//# sourceMappingURL=onboarding.d.ts.map