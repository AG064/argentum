/**
 * Mobile Push Notifications Feature
 *
 * Send push notifications to mobile devices via Firebase Cloud Messaging (FCM)
 * and Apple Push Notification service (APNs). Device registry with token management.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Platform types */
export type PushPlatform = 'android' | 'ios';
/** Registered device */
export interface Device {
    id: string;
    platform: PushPlatform;
    pushToken: string;
    userId?: string;
    metadata?: Record<string, string>;
    lastSeen?: number;
    createdAt: number;
    enabled: boolean;
}
/** Push notification payload */
export interface PushNotification {
    title: string;
    body: string;
    data?: Record<string, string>;
    url?: string;
    imageUrl?: string;
    sound?: string;
    badge?: number;
    priority?: 'high' | 'normal';
    ttl?: number;
}
/** Send result */
export interface SendResult {
    success: boolean;
    deviceId: string;
    messageId?: string;
    error?: string;
    platform: PushPlatform;
}
/** Feature configuration */
export interface MobilePushConfig {
    dbPath?: string;
    fcmServerKey?: string;
    fcmSenderId?: string;
    apnsKeyPath?: string;
    apnsKeyId?: string;
    apnsTeamId?: string;
    apnsBundleId?: string;
    maxLogEntries?: number;
}
/**
 * MobilePush — push notification service for Android and iOS.
 *
 * Provides:
 * - Device registration and management
 * - Push sending via FCM/APNs structure
 * - Platform-specific payload handling
 * - Notification logging and statistics
 * - Broadcast to all devices or platform-specific
 *
 * Real push sending is not implemented (requires FCM/APNs credentials).
 */
declare class MobilePushFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Check if at least one push provider is configured.
     */
    isConfigured(): boolean;
    /**
     * Register a device for push notifications.
     *
     * @param deviceId - Unique device identifier
     * @param platform - 'android' or 'ios'
     * @param pushToken - FCM token or APNs device token
     * @param userId - Optional user ID for grouping
     * @param metadata - Optional additional device info
     * @returns The registered Device object
     */
    register(deviceId: string, platform: PushPlatform, pushToken: string, userId?: string, metadata?: Record<string, string>): Device;
    /**
     * Unregister a device.
     */
    unregister(deviceId: string): boolean;
    /**
     * Get a device by ID.
     */
    getDevice(deviceId: string): Device | null;
    /**
     * List all registered devices.
     *
     * @param platform - Filter by platform (optional)
     * @param userId - Filter by user ID (optional)
     */
    listDevices(platform?: PushPlatform, userId?: string): Device[];
    /**
     * Send a push notification to a specific device.
     *
     * @param deviceId - Target device ID
     * @param notification - PushNotification payload
     * @returns SendResult indicating success/failure
     */
    send(deviceId: string, notification: PushNotification): Promise<SendResult>;
    /**
     * Broadcast a push notification to all devices or filtered set.
     *
     * @param notification - PushNotification payload
     * @param platform - Filter by platform (optional)
     * @param userId - Filter by user ID (optional)
     * @returns Array of SendResults
     */
    broadcast(notification: PushNotification, platform?: PushPlatform, userId?: string): Promise<SendResult[]>;
    /**
     * Get notification sending statistics.
     */
    getStats(): {
        totalSent: number;
        byPlatform: Record<string, number>;
        successes: number;
        failures: number;
        last24h: number;
    };
    /**
     * Update device lastSeen timestamp (call when device checks in).
     */
    touchDevice(deviceId: string): boolean;
    /**
     * Enable/disable a device.
     */
    setDeviceEnabled(deviceId: string, enabled: boolean): boolean;
    /** Initialize database and create tables */
    private initDatabase;
}
declare const _default: MobilePushFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map