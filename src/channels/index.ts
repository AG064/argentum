/**
 * Channels module — protocol adapters for external messaging platforms.
 *
 * Each channel is responsible for:
 *   - Receiving inbound messages from the platform
 *   - Routing them to the Agent
 *   - Sending responses back to the platform
 *
 * Supported channels:
 *   - Telegram (grammy-based bot)
 *   - Webchat (WebSocket + HTTP server)
 *   - Mobile (FCM push notifications)
 */

export type { ArgentumConfig } from '../core/config';
export type { Message } from '../core/llm-provider';
export { Logger, createLogger } from '../core/logger';
export { Agent } from '../index';
