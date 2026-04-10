/**
 * WhatsApp Bridge Feature
 *
 * WhatsApp Business API integration for sending/receiving messages.
 * Provides webhook handling for incoming messages and structured outbound sending.
 * All activity is logged to SQLite.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** WhatsApp configuration */
export interface WhatsAppConfig {
    apiToken: string;
    phoneNumberId: string;
    businessId?: string;
    webhookSecret?: string;
}
/** WhatsApp message (incoming/outgoing) */
export interface WhatsAppMessage {
    id: string;
    from: string;
    to: string;
    body: string;
    type: 'text' | 'image' | 'audio' | 'video' | 'document' | 'location' | 'contacts';
    timestamp: number;
    status: 'sent' | 'delivered' | 'read' | 'failed';
    mediaUrl?: string;
    mediaId?: string;
    metadata?: Record<string, unknown>;
}
/** Webhook payload (from WhatsApp) */
export interface WebhookPayload {
    object: string;
    entry: Array<{
        id: string;
        changes: Array<{
            value: {
                messaging_product: string;
                metadata: {
                    display_phone_number: string;
                    phone_number_id: string;
                };
                contacts?: Array<{
                    profile: {
                        name: string;
                    };
                    wa_id: string;
                }>;
                messages?: Array<{
                    id: string;
                    from: string;
                    timestamp: string;
                    type: string;
                    text?: {
                        body: string;
                    };
                    image?: {
                        id: string;
                        mime_type?: string;
                    };
                    audio?: {
                        id: string;
                        mime_type?: string;
                    };
                    video?: {
                        id: string;
                        mime_type?: string;
                    };
                    document?: {
                        id: string;
                        filename?: string;
                        mime_type?: string;
                    };
                    location?: {
                        latitude: string;
                        longitude: string;
                        name?: string;
                        address?: string;
                    };
                    contacts?: Array<{
                        name: {
                            formatted_name: string;
                        };
                        phones: Array<{
                            phone: string;
                        }>;
                    }>;
                }>;
                statuses?: Array<{
                    id: string;
                    status: 'sent' | 'delivered' | 'read' | 'failed';
                    timestamp: string;
                    error?: {
                        code: number;
                        title: string;
                        message_data?: unknown;
                    };
                }>;
            };
            field: string;
        }>;
    }>;
}
/** Feature configuration */
export interface WhatsAppBridgeConfig {
    dbPath?: string;
    webhookPath?: string;
    maxMessageHistory?: number;
    autoAck?: boolean;
}
/**
 * WhatsAppBridge — WhatsApp Business API integration.
 *
 * Provides:
 * - API token and phone number configuration
 * - Outbound message sending structure
 * - Inbound webhook parsing and handling
 * - Message history logging
 * - Status update handling
 *
 * Note: Real API calls are not implemented (would require HTTPS endpoint for webhooks).
 */
declare class WhatsAppBridgeFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private apiToken;
    private phoneNumberId;
    private businessId;
    private webhookSecret;
    private autoAck;
    private messageHandlers;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Configure WhatsApp Business API credentials.
     *
     * @param apiToken - Permanent access token
     * @param phoneNumberId - WhatsApp Business phone number ID
     * @param webhookSecret - Optional secret for webhook verification
     * @param businessId - Optional business ID
     */
    configure(apiToken: string, phoneNumberId: string, webhookSecret?: string, businessId?: string): void;
    /**
     * Check if WhatsApp is configured.
     */
    isConfigured(): boolean;
    /**
     * Register a handler for incoming messages.
     *
     * @param handler - Async function called for each inbound message
     */
    onMessage(handler: (msg: WhatsAppMessage) => Promise<void>): void;
    /**
     * Send a WhatsApp message (stub - would call WhatsApp API in real implementation).
     *
     * @param to - Recipient WhatsApp ID (phone number with country code)
     * @param body - Text message
     * @param type - Message type (default text)
     * @returns Sent WhatsAppMessage with status 'sent'
     */
    send(to: string, body: string, type?: WhatsAppMessage['type']): Promise<WhatsAppMessage>;
    /**
     * Get message history.
     *
     * @param limit - Max messages to return
     * @param direction - Filter by 'inbound', 'outbound', or undefined for all
     * @returns Messages ordered by timestamp descending
     */
    getMessages(limit?: number, direction?: 'inbound' | 'outbound'): WhatsAppMessage[];
    /**
     * Handle an incoming webhook from WhatsApp.
     *
     * @param payload - Raw webhook JSON payload
     * @param verifySignature - Optional signature verification function (returns boolean)
     * @returns true if webhook processed successfully
     */
    webhook(payload: WebhookPayload, _verifySignature?: (sig: string, body: string) => boolean): Promise<boolean>;
    /**
     * Get statistics about message activity.
     */
    getStats(): {
        total: number;
        inbound: number;
        outbound: number;
    };
    /** Log a message to database */
    private logMessage;
    /** Initialize database and create tables */
    private initDatabase;
}
declare const _default: WhatsAppBridgeFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map