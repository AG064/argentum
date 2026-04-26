/**
 * Webhooks Feature
 *
 * Receives and dispatches webhook events with signature verification,
 * retry logic, and event routing.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Webhook configuration */
export interface WebhooksConfig {
    enabled: boolean;
    port: number;
    path: string;
    secret: string;
    maxRetries: number;
    retryDelayMs: number;
}
/** Webhook event */
export interface WebhookEvent {
    id: string;
    source: string;
    type: string;
    payload: Record<string, unknown>;
    headers: Record<string, string>;
    timestamp: number;
    signature?: string;
}
/** Webhook subscription */
export interface WebhookSubscription {
    id: string;
    url: string;
    events: string[];
    secret: string;
    active: boolean;
    createdAt: number;
}
/** Webhook handler function */
export type WebhookHandler = (event: WebhookEvent) => Promise<void>;
/**
 * Webhooks feature — receive and dispatch webhook events.
 *
 * Provides HTTP endpoint for receiving webhooks, signature verification,
 * event routing to handlers, and outbound webhook delivery with retries.
 */
declare class WebhooksFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private handlers;
    private subscriptions;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Register a handler for a specific event type */
    on(eventType: string, handler: WebhookHandler): void;
    /** Verify webhook signature */
    verifySignature(payload: string, signature: string, secret: string): boolean;
    /** Process an incoming webhook event */
    processEvent(event: WebhookEvent): Promise<void>;
    /** Subscribe to outbound webhooks */
    subscribe(url: string, events: string[], secret: string): WebhookSubscription;
    /** Dispatch event to all matching subscribers */
    dispatch(event: WebhookEvent): Promise<void>;
    private isInternalHostname;
    private isPrivateIpv4;
    private isPrivateIpv6;
    private validateUrl;
    private validateDeliveryUrl;
    /** Deliver webhook with retry logic */
    private deliverWithRetry;
    /** Unsubscribe */
    unsubscribe(id: string): boolean;
}
declare const _default: WebhooksFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map