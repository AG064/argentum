/**
 * Telegram Channel for Argentum
 *
 * Full grammY-based Telegram bot with:
 * - Text and voice message handling
 * - Typing indicators
 * - User allowlisting
 * - Agent integration
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../core/plugin-loader';
import { type Agent } from '../index';
/** Telegram channel configuration */
export interface TelegramChannelConfig {
    enabled: boolean;
    token?: string;
    allowedUsers?: number[];
    allowedChats?: number[];
    blockedChats?: number[];
    parseMode?: 'HTML' | 'Markdown' | 'MarkdownV2';
    sendReasoning?: boolean;
}
/**
 * Telegram channel — real grammY bot.
 *
 * Handles message routing, typing indicators, voice transcription,
 * and user access control.
 */
declare class TelegramChannel implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private bot;
    private agent;
    private running;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Set the agent for message handling */
    setAgent(agent: Agent): void;
    /** Check if a user/chat is allowed */
    private isAllowed;
    /** Register bot command and message handlers */
    private registerHandlers;
    /** Split a message into chunks respecting Telegram limits */
    private splitMessage;
    private formatAgentResponse;
}
declare const _default: TelegramChannel;
export default _default;
//# sourceMappingURL=telegram.d.ts.map