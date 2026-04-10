"use strict";
/**
 * Mobile Push Notifications Feature
 *
 * Send push notifications to mobile devices via Firebase Cloud Messaging (FCM)
 * and Apple Push Notification service (APNs). Device registry with token management.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
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
class MobilePushFeature {
    meta = {
        name: 'mobile-push',
        version: '0.1.0',
        description: 'Mobile push notification service with FCM/APNs support',
        dependencies: [],
    };
    config;
    ctx;
    db;
    constructor() {
        this.config = {
            dbPath: './data/mobile-push.db',
            fcmServerKey: '',
            fcmSenderId: '',
            apnsKeyPath: '',
            apnsKeyId: '',
            apnsTeamId: '',
            apnsBundleId: '',
            maxLogEntries: 50000,
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            dbPath: config['dbPath'] ?? this.config['dbPath'],
            fcmServerKey: config['fcmServerKey'] ?? this.config['fcmServerKey'],
            fcmSenderId: config['fcmSenderId'] ?? this.config['fcmSenderId'],
            apnsKeyPath: config['apnsKeyPath'] ?? this.config['apnsKeyPath'],
            apnsKeyId: config['apnsKeyId'] ?? this.config['apnsKeyId'],
            apnsTeamId: config['apnsTeamId'] ?? this.config['apnsTeamId'],
            apnsBundleId: config['apnsBundleId'] ?? this.config['apnsBundleId'],
            maxLogEntries: config['maxLogEntries'] ?? this.config['maxLogEntries'],
        };
        this.initDatabase();
    }
    async start() {
        this.ctx.logger.info('MobilePush active', {
            dbPath: this.config.dbPath,
            configured: this.isConfigured(),
        });
    }
    async stop() {
        if (this.db) {
            this.db.close();
            this.ctx.logger.info('MobilePush stopped');
        }
    }
    async healthCheck() {
        try {
            const deviceCount = this.db.prepare('SELECT COUNT(*) as c FROM devices WHERE enabled = 1').get().c;
            const recentSends = this.db
                .prepare(`
        SELECT COUNT(*) as c FROM notification_logs
        WHERE sent_at > ?
      `)
                .get(Date.now() - 3600000).c;
            return {
                healthy: true,
                details: {
                    registeredDevices: deviceCount,
                    sentLastHour: recentSends,
                    configured: this.isConfigured(),
                },
            };
        }
        catch (err) {
            return {
                healthy: false,
                message: err instanceof Error ? err.message : String(err),
            };
        }
    }
    /**
     * Check if at least one push provider is configured.
     */
    isConfigured() {
        return !!(this.config.fcmServerKey ||
            (this.config.apnsKeyPath && this.config.apnsTeamId && this.config.apnsBundleId));
    }
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
    register(deviceId, platform, pushToken, userId, metadata) {
        const now = Date.now();
        // Upsert
        const existing = this.db.prepare('SELECT * FROM devices WHERE id = ?').get(deviceId);
        if (existing) {
            this.db
                .prepare(`
        UPDATE devices SET platform = ?, push_token = ?, user_id = ?, metadata = ?, last_seen = ?, enabled = 1
        WHERE id = ?
      `)
                .run(platform, pushToken, userId ?? null, metadata ? JSON.stringify(metadata) : null, now, deviceId);
        }
        else {
            this.db
                .prepare(`
        INSERT INTO devices (id, platform, push_token, user_id, metadata, last_seen, enabled, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `)
                .run(deviceId, platform, pushToken, userId ?? null, metadata ? JSON.stringify(metadata) : null, now, 1, now);
        }
        const device = {
            id: deviceId,
            platform,
            pushToken,
            userId,
            metadata,
            lastSeen: now,
            enabled: true,
            createdAt: existing ? 0 : now, // We'd fetch created_at if needed
        };
        this.ctx.logger.info('Device registered', { deviceId, platform, userId });
        return device;
    }
    /**
     * Unregister a device.
     */
    unregister(deviceId) {
        const result = this.db.prepare('DELETE FROM devices WHERE id = ?').run(deviceId);
        if (result.changes > 0) {
            this.db.prepare('DELETE FROM notification_logs WHERE device_id = ?').run(deviceId);
            this.ctx.logger.info('Device unregistered', { deviceId });
            return true;
        }
        return false;
    }
    /**
     * Get a device by ID.
     */
    getDevice(deviceId) {
        const row = this.db.prepare('SELECT * FROM devices WHERE id = ?').get(deviceId);
        if (!row)
            return null;
        return {
            id: row.id,
            platform: row.platform,
            pushToken: row.push_token,
            userId: row.user_id ?? undefined,
            metadata: row.metadata ? JSON.parse(row.metadata) : undefined,
            lastSeen: row.last_seen ?? undefined,
            enabled: row.enabled === 1,
            createdAt: row.created_at,
        };
    }
    /**
     * List all registered devices.
     *
     * @param platform - Filter by platform (optional)
     * @param userId - Filter by user ID (optional)
     */
    listDevices(platform, userId) {
        let query = 'SELECT * FROM devices WHERE enabled = 1';
        const params = [];
        if (platform) {
            query += ' AND platform = ?';
            params.push(platform);
        }
        if (userId) {
            query += ' AND user_id = ?';
            params.push(userId);
        }
        query += ' ORDER BY last_seen DESC';
        const rows = this.db.prepare(query).all(...params);
        return rows.map((row) => ({
            id: row.id,
            platform: row.platform,
            pushToken: row.push_token,
            userId: row.user_id ?? undefined,
            metadata: row.metadata ? JSON.parse(row.metadata) : undefined,
            lastSeen: row.last_seen ?? undefined,
            enabled: row.enabled === 1,
            createdAt: row.created_at,
        }));
    }
    /**
     * Send a push notification to a specific device.
     *
     * @param deviceId - Target device ID
     * @param notification - PushNotification payload
     * @returns SendResult indicating success/failure
     */
    async send(deviceId, notification) {
        const device = this.getDevice(deviceId);
        if (!device) {
            return { success: false, deviceId, platform: 'android', error: 'Device not registered' };
        }
        // In real implementation:
        // - For Android: send to FCM endpoint with server key
        // - For iOS: send to APNs with JWT token
        // Here we just log to database as "sent"
        const messageId = `push-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
        const now = Date.now();
        this.db
            .prepare(`
      INSERT INTO notification_logs (id, device_id, title, body, data, url, image_url, sound, badge, priority, ttl, sent_at, success)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `)
            .run(messageId, deviceId, notification.title, notification.body, notification.data ? JSON.stringify(notification.data) : null, notification.url ?? null, notification.imageUrl ?? null, notification.sound ?? null, notification.badge ?? null, notification.priority ?? 'normal', notification.ttl ?? null, now, 1);
        this.ctx.logger.info('Push notification sent (stub)', {
            deviceId,
            platform: device.platform,
            title: notification.title,
        });
        return {
            success: true,
            deviceId,
            messageId,
            platform: device.platform,
        };
    }
    /**
     * Broadcast a push notification to all devices or filtered set.
     *
     * @param notification - PushNotification payload
     * @param platform - Filter by platform (optional)
     * @param userId - Filter by user ID (optional)
     * @returns Array of SendResults
     */
    async broadcast(notification, platform, userId) {
        const devices = this.listDevices(platform, userId);
        const results = [];
        this.ctx.logger.info('Broadcasting push', { targetCount: devices.length, platform, userId });
        for (const device of devices) {
            const result = await this.send(device.id, notification);
            results.push(result);
        }
        return results;
    }
    /**
     * Get notification sending statistics.
     */
    getStats() {
        const total = this.db.prepare('SELECT COUNT(*) as c FROM notification_logs').get().c;
        const successes = this.db.prepare('SELECT COUNT(*) as c FROM notification_logs WHERE success = 1').get().c;
        const failures = total - successes;
        const last24h = this.db
            .prepare('SELECT COUNT(*) as c FROM notification_logs WHERE sent_at > ?')
            .get(Date.now() - 86400000).c;
        const platformRows = this.db
            .prepare(`
      SELECT d.platform, COUNT(l.id) as count
      FROM notification_logs l
      JOIN devices d ON l.device_id = d.id
      GROUP BY d.platform
    `)
            .all();
        const byPlatform = {};
        for (const row of platformRows) {
            byPlatform[row.platform] = row.count;
        }
        return {
            totalSent: total,
            byPlatform,
            successes,
            failures,
            last24h,
        };
    }
    /**
     * Update device lastSeen timestamp (call when device checks in).
     */
    touchDevice(deviceId) {
        const result = this.db
            .prepare('UPDATE devices SET last_seen = ? WHERE id = ?')
            .run(Date.now(), deviceId);
        return result.changes > 0;
    }
    /**
     * Enable/disable a device.
     */
    setDeviceEnabled(deviceId, enabled) {
        const result = this.db
            .prepare('UPDATE devices SET enabled = ? WHERE id = ?')
            .run(enabled ? 1 : 0, deviceId);
        if (result.changes > 0) {
            this.ctx.logger.info('Device enabled state changed', { deviceId, enabled });
            return true;
        }
        return false;
    }
    /** Initialize database and create tables */
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('synchronous = NORMAL');
        this.db.pragma('foreign_keys = ON');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS devices (
        id TEXT PRIMARY KEY,
        platform TEXT NOT NULL CHECK(platform IN ('android', 'ios')),
        push_token TEXT NOT NULL,
        user_id TEXT,
        metadata TEXT,
        last_seen INTEGER,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS notification_logs (
        id TEXT PRIMARY KEY,
        device_id TEXT NOT NULL,
        title TEXT NOT NULL,
        body TEXT NOT NULL,
        data TEXT,
        url TEXT,
        image_url TEXT,
        sound TEXT,
        badge INTEGER,
        priority TEXT DEFAULT 'normal',
        ttl INTEGER,
        sent_at INTEGER NOT NULL,
        success INTEGER NOT NULL DEFAULT 1,
        FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_devices_enabled ON devices(enabled);
      CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
      CREATE INDEX IF NOT EXISTS idx_logs_sent_at ON notification_logs(sent_at DESC);
      CREATE INDEX IF NOT EXISTS idx_logs_device ON notification_logs(device_id);
    `);
    }
}
exports.default = new MobilePushFeature();
//# sourceMappingURL=index.js.map