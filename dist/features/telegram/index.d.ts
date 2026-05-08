/**
 * Argentum Telegram Integration
 *
 * Based on OpenClaw's Telegram channel pattern:
 * - Grammy bot framework
 * - Token-based auth
 * - User/group allowlists
 * - Markdown formatting
 * - Reaction support
 */
import { type FeatureContext } from '../../core/plugin-loader';
export interface TelegramConfig {
    enabled: boolean;
    botToken: string;
    allowFrom: string[];
    groupPolicy: 'allowlist' | 'open' | 'disabled';
    groups: Record<string, {
        requireMention: boolean;
    }>;
    dmPolicy: 'pairing' | 'open' | 'disabled';
    streaming: 'full' | 'partial' | 'disabled';
    reactionNotifications: 'all' | 'mentions' | 'disabled';
    reactionLevel: 'minimal' | 'standard' | 'verbose';
    markdown: {
        tables: 'code' | 'markdown' | 'plain';
    };
}
interface TelegramMessage {
    id: number;
    chatId: number;
    userId: number;
    text: string;
    timestamp: number;
    replyTo?: number;
}
declare class TelegramFeature {
    readonly meta: {
        name: string;
        version: string;
        description: string;
        dependencies: string[];
    };
    private config;
    private bot;
    private ctx;
    private db;
    private messageHandlers;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    private setupHandlers;
    private isAllowed;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<{
        healthy: boolean;
        details: Record<string, unknown>;
    }>;
    onMessage(handler: (msg: TelegramMessage) => Promise<void>): void;
    sendMessage(chatId: number | string, text: string, options?: {
        parseMode?: 'Markdown' | 'MarkdownV2' | 'HTML';
        replyTo?: number;
    }): Promise<void>;
    sendPhoto(chatId: number | string, photo: string | Buffer, caption?: string): Promise<void>;
    sendVoice(chatId: number | string, voice: Buffer, caption?: string): Promise<void>;
    generatePairingCode(): string;
    listAllowedUsers(): string[];
    addUser(userId: string): void;
    removeUser(userId: string): void;
}
declare const _default: TelegramFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map