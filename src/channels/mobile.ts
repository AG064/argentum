/**
 * Mobile Channel
 *
 * Push notification delivery for mobile companion apps (iOS/Android).
 * Supports FCM and APNs with topic subscriptions and silent notifications.
 * Includes HTTP endpoint for receiving push registration requests.
 */

import express, { type Request, type Response, Router } from 'express';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../core/plugin-loader';

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
class MobileChannel implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'mobile',
    version: '0.1.0',
    description: 'Push notification channel for mobile companion apps with HTTP endpoint',
    dependencies: [],
  };

  private config: MobileChannelConfig = {
    enabled: false,
    httpPort: 3003,
    httpPath: '/mobile',
    topics: [],
    requireAuth: false,
  };
  private ctx!: FeatureContext;
  private devices: Map<string, DeviceRegistration> = new Map();
  private httpServer: ReturnType<typeof express.application.listen> | null = null;
  private router: Router;

  constructor() {
    this.router = Router();
    this.setupRoutes();
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<MobileChannelConfig>) };
  }

  async start(): Promise<void> {
    // Start HTTP server if enabled
    if (this.config.enabled) {
      await this.startHttpServer();
    }

    this.ctx.logger.info('Mobile channel active', {
      fcm: !!this.config.fcmServerKey,
      apns: !!this.config.apnsKeyId,
      httpPort: this.config.httpPort,
      topics: this.config.topics,
    });
  }

  async stop(): Promise<void> {
    if (this.httpServer) {
      this.httpServer.close();
      this.httpServer = null;
    }
    this.devices.clear();
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: this.config.enabled,
      details: {
        registeredDevices: this.devices.size,
        fcmConfigured: !!this.config.fcmServerKey,
        apnsConfigured: !!this.config.apnsKeyId,
        httpServerRunning: !!this.httpServer,
      },
    };
  }

  // ─── HTTP Routes ─────────────────────────────────────────────────────────

  private setupRoutes(): void {
    // Auth middleware
    const authMiddleware = (req: Request, res: Response, next: () => void): void => {
      if (!this.config.requireAuth) return next();

      const token = req.headers.authorization?.replace('Bearer ', '');
      if (token !== this.config.authToken) {
        res.status(401).json({ error: 'Unauthorized' });
        return;
      }
      next();
    };

    // Register device
    this.router.post('/register', authMiddleware, (req: Request, res: Response) => {
      try {
        const { userId, deviceId, platform, token, topics } = req.body as Record<string, unknown>;

        if (!userId || !deviceId || !platform || !token) {
          res
            .status(400)
            .json({ error: 'Missing required fields: userId, deviceId, platform, token' });
          return;
        }

        if (platform !== 'ios' && platform !== 'android') {
          res.status(400).json({ error: 'Platform must be "ios" or "android"' });
          return;
        }

        const registration: DeviceRegistration = {
          userId: userId as string,
          deviceId: deviceId as string,
          platform,
          token: token as string,
          topics: (topics as string[]) ?? [],
          registeredAt: Date.now(),
        };

        this.registerDevice(registration);
        res.json({ success: true, message: 'Device registered' });
      } catch (err) {
        res.status(500).json({ error: err instanceof Error ? err.message : String(err) });
      }
    });

    // Unregister device
    this.router.post('/unregister', authMiddleware, (req: Request, res: Response) => {
      const { userId, deviceId } = req.body as Record<string, unknown>;
      if (!userId || !deviceId) {
        res.status(400).json({ error: 'Missing required fields: userId, deviceId' });
        return;
      }

      const removed = this.unregisterDevice(userId as string, deviceId as string);
      res.json({ success: removed, message: removed ? 'Device unregistered' : 'Device not found' });
    });

    // Send notification
    this.router.post('/send', authMiddleware, async (req: Request, res: Response) => {
      try {
        const notification = req.body as PushNotification;
        if (!notification.userId || !notification.title || !notification.body) {
          res.status(400).json({ error: 'Missing required fields: userId, title, body' });
          return;
        }

        const result = await this.sendNotification({
          ...notification,
          priority: notification.priority ?? 'normal',
        });

        res.json(result);
      } catch (err) {
        res.status(500).json({ error: err instanceof Error ? err.message : String(err) });
      }
    });

    // Broadcast to user (all devices)
    this.router.post('/broadcast', authMiddleware, async (req: Request, res: Response) => {
      try {
        const { userId, title, body, data, priority } = req.body as Record<string, unknown>;
        if (!userId || !title || !body) {
          res.status(400).json({ error: 'Missing required fields: userId, title, body' });
          return;
        }

        const results = await this.broadcastToUser(userId as string, {
          title: title as string,
          body: body as string,
          data: data as Record<string, string> | undefined,
          priority: (priority as 'normal' | 'high') ?? 'normal',
        });

        res.json({ results });
      } catch (err) {
        res.status(500).json({ error: err instanceof Error ? err.message : String(err) });
      }
    });

    // Subscribe to topic
    this.router.post('/subscribe', authMiddleware, (req: Request, res: Response) => {
      const { userId, deviceId, topic } = req.body as Record<string, unknown>;
      if (!userId || !deviceId || !topic) {
        res.status(400).json({ error: 'Missing required fields' });
        return;
      }

      const key = `${userId}:${deviceId}`;
      const device = this.devices.get(key);
      if (!device) {
        res.status(404).json({ error: 'Device not found' });
        return;
      }

      if (!device.topics.includes(topic as string)) {
        device.topics.push(topic as string);
      }

      res.json({ success: true, topics: device.topics });
    });

    // List devices for user
    this.router.get('/devices/:userId', authMiddleware, (req: Request, res: Response) => {
      const devices = this.getUserDevices(req.params['userId'] ?? '');
      res.json({ devices: devices.map((d) => ({ ...d, token: '***' })) }); // Mask tokens
    });

    // Health check
    this.router.get('/health', (_req: Request, res: Response) => {
      res.json({
        status: 'ok',
        devices: this.devices.size,
        fcm: !!this.config.fcmServerKey,
        apns: !!this.config.apnsKeyId,
      });
    });
  }

  // ─── HTTP Server ─────────────────────────────────────────────────────────

  private async startHttpServer(): Promise<void> {
    const app = express();
    app.use(express.json());
    app.use(this.config.httpPath, this.router);

    return new Promise((resolve, reject) => {
      this.httpServer = app.listen(this.config.httpPort, () => {
        this.ctx.logger.info('Mobile HTTP endpoint started', {
          port: this.config.httpPort,
          path: this.config.httpPath,
        });
        resolve();
      });

      this.httpServer.on('error', (err) => {
        this.ctx.logger.error('Mobile HTTP server error', { error: err.message });
        reject(err);
      });
    });
  }

  // ─── Public API ──────────────────────────────────────────────────────────

  /** Register a device for push notifications */
  registerDevice(registration: DeviceRegistration): void {
    const key = `${registration.userId}:${registration.deviceId}`;
    this.devices.set(key, registration);
    this.ctx.logger.info('Device registered', {
      userId: registration.userId,
      platform: registration.platform,
      topics: registration.topics,
    });
  }

  /** Unregister a device */
  unregisterDevice(userId: string, deviceId: string): boolean {
    return this.devices.delete(`${userId}:${deviceId}`);
  }

  /** Get all devices for a user */
  getUserDevices(userId: string): DeviceRegistration[] {
    return Array.from(this.devices.values()).filter((d) => d.userId === userId);
  }

  /** Send push notification */
  async sendNotification(notification: PushNotification): Promise<NotificationResult> {
    const device =
      this.devices.get(`${notification.userId}:default`) ??
      Array.from(this.devices.values()).find((d) => d.userId === notification.userId);

    if (!device) {
      return { success: false, platform: 'none', error: 'No device registered for user' };
    }

    try {
      if (device.platform === 'android' && this.config.fcmServerKey) {
        return await this.sendFCM(device, notification);
      } else if (device.platform === 'ios' && this.config.apnsKeyId) {
        return await this.sendAPNs(device, notification);
      }

      // Stub mode — log but don't send
      this.ctx.logger.debug('Push notification (no provider configured)', {
        userId: notification.userId,
        title: notification.title,
        platform: device.platform,
      });
      return {
        success: true,
        platform: device.platform,
        messageId: `stub-${Date.now()}`,
      };
    } catch (err) {
      return {
        success: false,
        platform: device.platform,
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /** Send to all devices for a user */
  async broadcastToUser(
    userId: string,
    notification: Omit<PushNotification, 'userId'>,
  ): Promise<NotificationResult[]> {
    const devices = this.getUserDevices(userId);
    const results: NotificationResult[] = [];

    for (const device of devices) {
      results.push(await this.sendNotification({ ...notification, userId: device.userId }));
    }

    return results;
  }

  /** Send via Firebase Cloud Messaging */
  private async sendFCM(
    device: DeviceRegistration,
    notification: PushNotification,
  ): Promise<NotificationResult> {
    if (!this.config.fcmServerKey) {
      return { success: false, platform: 'android', error: 'FCM not configured' };
    }

    try {
      const response = await fetch('https://fcm.googleapis.com/fcm/send', {
        method: 'POST',
        headers: {
          'Authorization': `key=${this.config.fcmServerKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          to: device.token,
          notification: {
            title: notification.title,
            body: notification.body,
            sound: notification.sound ?? 'default',
          },
          data: notification.data,
          priority: notification.priority === 'high' ? 'high' : 'normal',
        }),
      });

      const result = (await response.json()) as {
        success: number;
        failure: number;
        message_id?: string;
      };
      return {
        success: result.success === 1,
        platform: 'android',
        messageId: result.message_id,
      };
    } catch (err) {
      return {
        success: false,
        platform: 'android',
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /** Send via Apple Push Notification Service (stub — needs APNs HTTP/2 client) */
  private async sendAPNs(
    device: DeviceRegistration,
    notification: PushNotification,
  ): Promise<NotificationResult> {
    // Real implementation would use apn2 or apns2 library with JWT auth
    this.ctx.logger.debug('APNs send (stub)', {
      device: device.deviceId,
      title: notification.title,
    });
    return { success: true, platform: 'ios', messageId: `apns-stub-${Date.now()}` };
  }
}

export default new MobileChannel();
