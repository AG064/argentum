/**
 * Slack Integration Feature
 *
 * Slack bot integration using Bolt.js patterns. Supports sending messages
 * and receiving events via Events API and Interactivity. Stub implementation.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Slack message */
export interface SlackMessage {
    ts: string;
    channel: string;
    user?: string;
    username?: string;
    text: string;
    blocks?: unknown[];
    attachments?: unknown[];
    threadTs?: string;
    botId?: string;
}
/** Slack event payload */
export interface SlackEventPayload {
    token: string;
    teamId: string;
    apiAppId: string;
    event: {
        type: string;
        user?: string;
        channel?: string;
        text?: string;
        ts: string;
        threadTs?: string;
        botId?: string;
        [key: string]: unknown;
    };
    type: string;
    eventId: string;
    eventTime: number;
}
/** Slack interaction payload (buttons, modals, etc.) */
export interface SlackInteraction {
    type: string;
    user: {
        id: string;
        username: string;
    };
    channel?: {
        id: string;
        name: string;
    };
    message?: SlackMessage;
    actions?: Array<{
        actionId: string;
        value: string;
        blockId: string;
    }>;
    submission?: Record<string, unknown>;
    view?: unknown;
}
/** Bot configuration */
export interface SlackBotConfig {
    botToken: string;
    signingSecret: string;
    appToken?: string;
    socketMode?: boolean;
    appLevelToken?: string;
}
/** Feature configuration */
export interface SlackIntegrationConfig {
    dbPath?: string;
    tokenEnvVar?: string;
    secretEnvVar?: string;
}
/**
 * SlackIntegration — Slack bot with Events API support.
 *
 * Provides:
 * - Token and signing secret configuration
 * - Message sending to channels/DMs
 * - Event handling (message, reaction, etc.)
 * - Interactivity handling (buttons, modals)
 * - Request verification (signing secret)
 *
 * Real Slack connection (HTTP server + Events API) is not implemented.
 */
declare class SlackIntegrationFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private botToken;
    private signingSecret;
    private appToken;
    private socketMode;
    private eventHandlers;
    private interactionHandlers;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Configure Slack bot credentials.
     *
     * @param botToken - Bot User OAuth Token (xoxb-...)
     * @param signingSecret - Signing Secret for Events API verification
     * @param appToken - App-Level Token for Socket Mode (xapp-..., optional)
     * @param socketMode - Use Socket Mode instead of HTTP Events API
     */
    configure(botToken: string, signingSecret: string, appToken?: string, socketMode?: boolean): void;
    /**
     * Check if bot is configured.
     */
    isConfigured(): boolean;
    /**
     * Register an event handler.
     *
     * @param eventType - Slack event type (e.g., 'message', 'reaction_added')
     * @param handler - Async function to process event
     */
    onEvent(eventType: string, handler: (event: SlackEventPayload['event']) => Promise<void>): void;
    /**
     * Register an interaction handler (buttons, modals, shortcuts).
     */
    onInteraction(handler: (interaction: SlackInteraction) => Promise<void>): void;
    /**
     * Send a message to a Slack channel (stub).
     *
     * @param channel - Channel ID (Cxxxxxxxx) or DM (Dxxxxxxxx)
     * @param text - Plain text message (up to 4000 chars)
     * @param blocks - Optional Block Kit UI blocks
     * @returns The sent message with ts
     */
    sendMessage(channel: string, text: string, blocks?: unknown[]): Promise<SlackMessage>;
    /**
     * Send an ephemeral message (only visible to user).
     */
    sendEphemeral(channel: string, user: string, _text: string, _blocks?: unknown[]): Promise<{
        ok: boolean;
    }>;
    /**
     * Process an incoming Slack event (stub).
     * Would be called by HTTP endpoint in real implementation.
     */
    receiveEvent(payload: SlackEventPayload): Promise<boolean>;
    /**
     * Process an incoming interaction (buttons, modals).
     */
    receiveInteraction(interaction: SlackInteraction): Promise<boolean>;
    /**
     * Verify a Slack request signature (would use crypto in real impl).
     */
    verifySignature(_signingSecret: string, _timestamp: string, _signature: string, _body: string): boolean;
    /**
     * Get bot user info (stub - would call auth.test API).
     */
    getBotInfo(): Promise<{
        userId: string;
        teamId: string;
        username: string;
    }>;
    /**
     * Get statistics.
     */
    getStats(): {
        configured: boolean;
        eventsReceived: number;
        messagesSent: number;
        interactions: number;
        activeEventHandlers: number;
    };
    /** Log sent message */
    private logSentMessage;
    /** Log received event */
    private logReceivedEvent;
    /** Log received interaction */
    private logReceivedInteraction;
    /** Initialize database and create tables */
    private initDatabase;
}
declare const _default: SlackIntegrationFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map