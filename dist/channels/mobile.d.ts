/**
 * Mobile Channel
 *
 * Push notification delivery for mobile companion apps (iOS/Android).
 * Supports FCM and APNs with topic subscriptions and silent notifications.
 * Includes HTTP endpoint for receiving push registration requests.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../core/plugin-loader';
/** Mobile channel configuration */
export interface MobileChannelConfig {
    enabled: boolean;
    httpPort: number;
    httpPath: string;
    fcmServerKey?: string;
    apnsKeyId?: string;
    apnsTeamId?: string;
    apnsBundleId?: string;
    topics: string[];
    requireAuth: boolean;
    authToken?: string;
}
/** Push notification payload */
export interface PushNotification {
    userId: string;
    title: string;
    body: string;
    data?: Record<string, string>;
    badge?: number;
    sound?: string;
    topic?: string;
    priority: 'normal' | 'high';
}
/** Device registration */
export interface DeviceRegistration {
    userId: string;
    deviceId: string;
    platform: 'ios' | 'android';
    token: string;
    topics: string[];
    registeredAt: number;
}
/** Notification result */
export interface NotificationResult {
    success: boolean;
    platform: string;
    messageId?: string;
    error?: string;
}
/**
 * Mobile channel — push notification delivery for companion apps.
 *
 * Manages device registrations, topic subscriptions, and sends
 * push notifications via FCM (Android) and APNs (iOS).
 * Provides HTTP endpoint for device registration and notification triggers.
 */
declare class MobileChannel implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private devices;
    private httpServer;
    private router;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    private setupRoutes;
    private startHttpServer;
    /** Register a device for push notifications */
    registerDevice(registration: DeviceRegistration): void;
    /** Unregister a device */
    unregisterDevice(userId: string, deviceId: string): boolean;
    /** Get all devices for a user */
    getUserDevices(userId: string): DeviceRegistration[];
    /** Send push notification */
    sendNotification(notification: PushNotification): Promise<NotificationResult>;
    /** Send to all devices for a user */
    broadcastToUser(userId: string, notification: Omit<PushNotification, 'userId'>): Promise<NotificationResult[]>;
    /** Send via Firebase Cloud Messaging */
    private sendFCM;
    /** Send via Apple Push Notification Service (stub — needs APNs HTTP/2 client) */
    private sendAPNs;
}
declare const _default: MobileChannel;
export default _default;
//# sourceMappingURL=mobile.d.ts.map