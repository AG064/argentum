/**
 * Mobile Push Notifications Feature
 *
 * Send push notifications to mobile devices via Firebase Cloud Messaging (FCM)
 * and Apple Push Notification service (APNs). Device registry with token management.
 */

import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** Platform types */
export type PushPlatform = 'android' | 'ios';

/** Registered device */
export interface Device {
  id: string; // User/chosen device identifier
  platform: PushPlatform;
  pushToken: string; // FCM token or APNs device token
  userId?: string; // Optional user association
  metadata?: Record<string, string>;
  lastSeen?: number; // Last activity timestamp
  createdAt: number;
  enabled: boolean;
}

/** Push notification payload */
export interface PushNotification {
  title: string;
  body: string;
  data?: Record<string, string>; // Custom key-value pairs
  url?: string; // Deep link
  imageUrl?: string;
  sound?: string;
  badge?: number; // iOS badge count
  priority?: 'high' | 'normal';
  ttl?: number; // Time-to-live in seconds
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
  fcmServerKey?: string; // Firebase Cloud Messaging
  fcmSenderId?: string;
  apnsKeyPath?: string; // APNs .p8 key file path
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
class MobilePushFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'mobile-push',
    version: '0.0.2',
    description: 'Mobile push notification service with FCM/APNs support',
    dependencies: [],
  };

  private config: Required<MobilePushConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;

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

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      dbPath: (config['dbPath'] as string) ?? this.config['dbPath'],
      fcmServerKey: (config['fcmServerKey'] as string) ?? this.config['fcmServerKey'],
      fcmSenderId: (config['fcmSenderId'] as string) ?? this.config['fcmSenderId'],
      apnsKeyPath: (config['apnsKeyPath'] as string) ?? this.config['apnsKeyPath'],
      apnsKeyId: (config['apnsKeyId'] as string) ?? this.config['apnsKeyId'],
      apnsTeamId: (config['apnsTeamId'] as string) ?? this.config['apnsTeamId'],
      apnsBundleId: (config['apnsBundleId'] as string) ?? this.config['apnsBundleId'],
      maxLogEntries: (config['maxLogEntries'] as number) ?? this.config['maxLogEntries'],
    };

    this.initDatabase();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('MobilePush active', {
      dbPath: this.config.dbPath,
      configured: this.isConfigured(),
    });
  }

  async stop(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.ctx.logger.info('MobilePush stopped');
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const deviceCount = (
        this.db.prepare('SELECT COUNT(*) as c FROM devices WHERE enabled = 1').get() as {
          c: number;
        }
      ).c;
      const recentSends = (
        this.db
          .prepare(
            `
        SELECT COUNT(*) as c FROM notification_logs
        WHERE sent_at > ?
      `,
          )
          .get(Date.now() - 3600000) as { c: number }
      ).c;

      return {
        healthy: true,
        details: {
          registeredDevices: deviceCount,
          sentLastHour: recentSends,
          configured: this.isConfigured(),
        },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /**
   * Check if at least one push provider is configured.
   */
  isConfigured(): boolean {
    return !!(
      this.config.fcmServerKey ||
      (this.config.apnsKeyPath && this.config.apnsTeamId && this.config.apnsBundleId)
    );
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
  register(
    deviceId: string,
    platform: PushPlatform,
    pushToken: string,
    userId?: string,
    metadata?: Record<string, string>,
  ): Device {
    const now = Date.now();

    // Upsert
    const existing = this.db.prepare('SELECT * FROM devices WHERE id = ?').get(deviceId) as
      | { id: string }
      | undefined;

    if (existing) {
      this.db
        .prepare(
          `
        UPDATE devices SET platform = ?, push_token = ?, user_id = ?, metadata = ?, last_seen = ?, enabled = 1
        WHERE id = ?
      `,
        )
        .run(
          platform,
          pushToken,
          userId ?? null,
          metadata ? JSON.stringify(metadata) : null,
          now,
          deviceId,
        );
    } else {
      this.db
        .prepare(
          `
        INSERT INTO devices (id, platform, push_token, user_id, metadata, last_seen, enabled, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `,
        )
        .run(
          deviceId,
          platform,
          pushToken,
          userId ?? null,
          metadata ? JSON.stringify(metadata) : null,
          now,
          1,
          now,
        );
    }

    const device: Device = {
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
  unregister(deviceId: string): boolean {
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
  getDevice(deviceId: string): Device | null {
    const row = this.db.prepare('SELECT * FROM devices WHERE id = ?').get(deviceId) as
      | {
          id: string;
          platform: string;
          push_token: string;
          user_id: string | null;
          metadata: string | null;
          last_seen: number | null;
          enabled: number;
          created_at: number;
        }
      | undefined;

    if (!row) return null;

    return {
      id: row.id,
      platform: row.platform as PushPlatform,
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
  listDevices(platform?: PushPlatform, userId?: string): Device[] {
    let query = 'SELECT * FROM devices WHERE enabled = 1';
    const params: (string | number)[] = [];

    if (platform) {
      query += ' AND platform = ?';
      params.push(platform);
    }
    if (userId) {
      query += ' AND user_id = ?';
      params.push(userId);
    }

    query += ' ORDER BY last_seen DESC';

    const rows = this.db.prepare(query).all(...params) as Array<{
      id: string;
      platform: string;
      push_token: string;
      user_id: string | null;
      metadata: string | null;
      last_seen: number | null;
      enabled: number;
      created_at: number;
    }>;

    return rows.map((row) => ({
      id: row.id,
      platform: row.platform as PushPlatform,
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
  async send(deviceId: string, notification: PushNotification): Promise<SendResult> {
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
      .prepare(
        `
      INSERT INTO notification_logs (id, device_id, title, body, data, url, image_url, sound, badge, priority, ttl, sent_at, success)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `,
      )
      .run(
        messageId,
        deviceId,
        notification.title,
        notification.body,
        notification.data ? JSON.stringify(notification.data) : null,
        notification.url ?? null,
        notification.imageUrl ?? null,
        notification.sound ?? null,
        notification.badge ?? null,
        notification.priority ?? 'normal',
        notification.ttl ?? null,
        now,
        1,
      );

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
  async broadcast(
    notification: PushNotification,
    platform?: PushPlatform,
    userId?: string,
  ): Promise<SendResult[]> {
    const devices = this.listDevices(platform, userId);
    const results: SendResult[] = [];

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
  getStats(): {
    totalSent: number;
    byPlatform: Record<string, number>;
    successes: number;
    failures: number;
    last24h: number;
  } {
    const total = (
      this.db.prepare('SELECT COUNT(*) as c FROM notification_logs').get() as { c: number }
    ).c;
    const successes = (
      this.db.prepare('SELECT COUNT(*) as c FROM notification_logs WHERE success = 1').get() as {
        c: number;
      }
    ).c;
    const failures = total - successes;
    const last24h = (
      this.db
        .prepare('SELECT COUNT(*) as c FROM notification_logs WHERE sent_at > ?')
        .get(Date.now() - 86400000) as { c: number }
    ).c;

    const platformRows = this.db
      .prepare(
        `
      SELECT d.platform, COUNT(l.id) as count
      FROM notification_logs l
      JOIN devices d ON l.device_id = d.id
      GROUP BY d.platform
    `,
      )
      .all() as Array<{ platform: string; count: number }>;

    const byPlatform: Record<string, number> = {};
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
  touchDevice(deviceId: string): boolean {
    const result = this.db
      .prepare('UPDATE devices SET last_seen = ? WHERE id = ?')
      .run(Date.now(), deviceId);
    return result.changes > 0;
  }

  /**
   * Enable/disable a device.
   */
  setDeviceEnabled(deviceId: string, enabled: boolean): boolean {
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
  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
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

export default new MobilePushFeature();
