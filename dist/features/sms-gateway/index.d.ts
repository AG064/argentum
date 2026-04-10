/**
 * SMS Gateway Feature
 *
 * Pluggable SMS sending architecture with provider abstraction.
 * Supports multiple providers (Twilio, Vonage, etc.) via plugin system.
 * All SMS activity is logged to SQLite.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** SMS provider interface */
export interface SmsProvider {
    /** Provider name (e.g., 'twilio', 'vonage') */
    name: string;
    /** Configure provider with API key/credentials */
    configure(credentials: Record<string, string>): void;
    /** Send SMS */
    send(to: string, message: string, from?: string): Promise<SmsResult>;
    /** Get account balance (if supported) */
    getBalance?(): Promise<number | null>;
}
/** SMS send result */
export interface SmsResult {
    success: boolean;
    messageId?: string;
    provider?: string;
    error?: string;
    timestamp: number;
}
/** SMS log entry */
export interface SmsLog {
    id: string;
    provider: string;
    to: string;
    from: string;
    body: string;
    messageId?: string;
    success: boolean;
    error?: string;
    sent_at: number;
}
/** Feature configuration */
export interface SmsGatewayConfig {
    dbPath?: string;
    defaultProvider?: string;
    maxLogEntries?: number;
}
/**
 * SmsGateway — modular SMS sending with pluggable providers.
 *
 * Provides:
 * - Provider plugin system
 * - SMS sending with logging
 * - Message history tracking
 * - Provider configuration management
 *
 * Providers are registered separately; this core manages routing and logging.
 */
declare class SmsGatewayFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private providers;
    private defaultProvider;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Register a provider plugin.
     *
     * @param provider - Provider instance implementing SmsProvider interface
     */
    registerProvider(provider: SmsProvider): void;
    /**
     * Unregister a provider.
     */
    unregisterProvider(name: string): boolean;
    /**
     * Configure a provider with credentials.
     *
     * @param providerName - Provider to configure
     * @param credentials - API key/secret or other credentials
     */
    configureProvider(providerName: string, credentials: Record<string, string>): void;
    /**
     * Set the default provider for send operations.
     */
    setDefaultProvider(name: string): boolean;
    /**
     * Send an SMS via configured provider.
     *
     * @param to - Recipient phone number (E.164 format)
     * @param message - SMS text
     * @param from - Sender ID/phone (optional, provider default if not set)
     * @param providerName - Specific provider to use (default if not specified)
     * @returns SmsResult with status and messageId if successful
     */
    send(to: string, message: string, from?: string, providerName?: string): Promise<SmsResult>;
    /**
     * Get SMS sending history.
     *
     * @param limit - Max entries to return (most recent first)
     * @param provider - Filter by specific provider (optional)
     * @returns Array of SMS logs
     */
    getHistory(limit?: number, provider?: string): SmsLog[];
    /**
     * Get sending statistics for a provider.
     *
     * @param providerName - Provider to get stats for
     * @param hoursBack - Lookback period in hours (default 24)
     */
    getProviderStats(providerName: string, hoursBack?: number): {
        sent: number;
        delivered: number;
        failed: number;
    };
    /**
     * List registered providers.
     */
    listProviders(): string[];
    /** Log SMS to database */
    private logSms;
    /** Initialize database and create tables */
    private initDatabase;
}
declare const _default: SmsGatewayFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map