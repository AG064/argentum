'use strict';
/**
 * Mobile Channel
 *
 * Push notification delivery for mobile companion apps (iOS/Android).
 * Supports FCM and APNs with topic subscriptions and silent notifications.
 * Includes HTTP endpoint for receiving push registration requests.
 */
var __createBinding =
  (this && this.__createBinding) ||
  (Object.create
    ? function (o, m, k, k2) {
        if (k2 === undefined) k2 = k;
        var desc = Object.getOwnPropertyDescriptor(m, k);
        if (!desc || ('get' in desc ? !m.__esModule : desc.writable || desc.configurable)) {
          desc = {
            enumerable: true,
            get: function () {
              return m[k];
            },
          };
        }
        Object.defineProperty(o, k2, desc);
      }
    : function (o, m, k, k2) {
        if (k2 === undefined) k2 = k;
        o[k2] = m[k];
      });
var __setModuleDefault =
  (this && this.__setModuleDefault) ||
  (Object.create
    ? function (o, v) {
        Object.defineProperty(o, 'default', { enumerable: true, value: v });
      }
    : function (o, v) {
        o['default'] = v;
      });
var __importStar =
  (this && this.__importStar) ||
  (function () {
    var ownKeys = function (o) {
      ownKeys =
        Object.getOwnPropertyNames ||
        function (o) {
          var ar = [];
          for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
          return ar;
        };
      return ownKeys(o);
    };
    return function (mod) {
      if (mod && mod.__esModule) return mod;
      var result = {};
      if (mod != null)
        for (var k = ownKeys(mod), i = 0; i < k.length; i++)
          if (k[i] !== 'default') __createBinding(result, mod, k[i]);
      __setModuleDefault(result, mod);
      return result;
    };
  })();
Object.defineProperty(exports, '__esModule', { value: true });
const express_1 = __importStar(require('express'));
/**
 * Mobile channel — push notification delivery for companion apps.
 *
 * Manages device registrations, topic subscriptions, and sends
 * push notifications via FCM (Android) and APNs (iOS).
 * Provides HTTP endpoint for device registration and notification triggers.
 */
class MobileChannel {
  constructor() {
    this.meta = {
      name: 'mobile',
      version: '0.0.3',
      description: 'Push notification channel for mobile companion apps with HTTP endpoint',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      httpPort: 3003,
      httpPath: '/mobile',
      topics: [],
      requireAuth: false,
    };
    this.devices = new Map();
    this.httpServer = null;
    this.router = (0, express_1.Router)();
    this.setupRoutes();
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
  }
  async start() {
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
  async stop() {
    if (this.httpServer) {
      this.httpServer.close();
      this.httpServer = null;
    }
    this.devices.clear();
  }
  async healthCheck() {
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
  setupRoutes() {
    // Auth middleware
    const authMiddleware = (req, res, next) => {
      if (!this.config.requireAuth) return next();
      const token = req.headers.authorization?.replace('Bearer ', '');
      if (token !== this.config.authToken) {
        res.status(401).json({ error: 'Unauthorized' });
        return;
      }
      next();
    };
    // Register device
    this.router.post('/register', authMiddleware, (req, res) => {
      try {
        const { userId, deviceId, platform, token, topics } = req.body;
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
        const registration = {
          userId: userId,
          deviceId: deviceId,
          platform: platform,
          token: token,
          topics: topics ?? [],
          registeredAt: Date.now(),
        };
        this.registerDevice(registration);
        res.json({ success: true, message: 'Device registered' });
      } catch (err) {
        res.status(500).json({ error: err instanceof Error ? err.message : String(err) });
      }
    });
    // Unregister device
    this.router.post('/unregister', authMiddleware, (req, res) => {
      const { userId, deviceId } = req.body;
      if (!userId || !deviceId) {
        res.status(400).json({ error: 'Missing required fields: userId, deviceId' });
        return;
      }
      const removed = this.unregisterDevice(userId, deviceId);
      res.json({ success: removed, message: removed ? 'Device unregistered' : 'Device not found' });
    });
    // Send notification
    this.router.post('/send', authMiddleware, async (req, res) => {
      try {
        const notification = req.body;
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
    this.router.post('/broadcast', authMiddleware, async (req, res) => {
      try {
        const { userId, title, body, data, priority } = req.body;
        if (!userId || !title || !body) {
          res.status(400).json({ error: 'Missing required fields: userId, title, body' });
          return;
        }
        const results = await this.broadcastToUser(userId, {
          title: title,
          body: body,
          data: data,
          priority: priority ?? 'normal',
        });
        res.json({ results });
      } catch (err) {
        res.status(500).json({ error: err instanceof Error ? err.message : String(err) });
      }
    });
    // Subscribe to topic
    this.router.post('/subscribe', authMiddleware, (req, res) => {
      const { userId, deviceId, topic } = req.body;
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
      if (!device.topics.includes(topic)) {
        device.topics.push(topic);
      }
      res.json({ success: true, topics: device.topics });
    });
    // List devices for user
    this.router.get('/devices/:userId', authMiddleware, (req, res) => {
      const devices = this.getUserDevices(req.params.userId);
      res.json({ devices: devices.map((d) => ({ ...d, token: '***' })) }); // Mask tokens
    });
    // Health check
    this.router.get('/health', (_req, res) => {
      res.json({
        status: 'ok',
        devices: this.devices.size,
        fcm: !!this.config.fcmServerKey,
        apns: !!this.config.apnsKeyId,
      });
    });
  }
  // ─── HTTP Server ─────────────────────────────────────────────────────────
  async startHttpServer() {
    const app = (0, express_1.default)();
    app.use(express_1.default.json());
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
  registerDevice(registration) {
    const key = `${registration.userId}:${registration.deviceId}`;
    this.devices.set(key, registration);
    this.ctx.logger.info('Device registered', {
      userId: registration.userId,
      platform: registration.platform,
      topics: registration.topics,
    });
  }
  /** Unregister a device */
  unregisterDevice(userId, deviceId) {
    return this.devices.delete(`${userId}:${deviceId}`);
  }
  /** Get all devices for a user */
  getUserDevices(userId) {
    return Array.from(this.devices.values()).filter((d) => d.userId === userId);
  }
  /** Send push notification */
  async sendNotification(notification) {
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
  async broadcastToUser(userId, notification) {
    const devices = this.getUserDevices(userId);
    const results = [];
    for (const device of devices) {
      results.push(await this.sendNotification({ ...notification, userId: device.userId }));
    }
    return results;
  }
  /** Send via Firebase Cloud Messaging */
  async sendFCM(device, notification) {
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
      const result = await response.json();
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
  async sendAPNs(device, notification) {
    // Real implementation would use apn2 or apns2 library with JWT auth
    this.ctx.logger.debug('APNs send (stub)', {
      device: device.deviceId,
      title: notification.title,
    });
    return { success: true, platform: 'ios', messageId: `apns-stub-${Date.now()}` };
  }
}
exports.default = new MobileChannel();
