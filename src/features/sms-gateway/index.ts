/**
 * SMS Gateway Feature
 *
 * Pluggable SMS sending architecture with provider abstraction.
 * Supports multiple providers (Twilio, Vonage, etc.) via plugin system.
 * All SMS activity is logged to SQLite.
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

/** SMS provider interface */
export interface SmsProvider {
  /** Provider name (e.g., 'twilio', 'vonage') */
  name: string;
  /** Configure provider with API key/credentials */
  configure(credentials: Record<string, string>): void;
  /** Send SMS */
  send(to: string, message: string, from?: string): Promise<SmsResult>;
  /** Get account balance (if supported) */
  getBalance?(): Promise<number | null>;
}

/** SMS send result */
export interface SmsResult {
  success: boolean;
  messageId?: string;
  provider?: string;
  error?: string;
  timestamp: number;
}

/** SMS log entry */
export interface SmsLog {
  id: string;
  provider: string;
  to: string;
  from: string;
  body: string;
  messageId?: string;
  success: boolean;
  error?: string;
  sent_at: number;
}

/** Feature configuration */
export interface SmsGatewayConfig {
  dbPath?: string;
  defaultProvider?: string;
  maxLogEntries?: number;
}

/**
 * SmsGateway — modular SMS sending with pluggable providers.
 *
 * Provides:
 * - Provider plugin system
 * - SMS sending with logging
 * - Message history tracking
 * - Provider configuration management
 *
 * Providers are registered separately; this core manages routing and logging.
 */
class SmsGatewayFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'sms-gateway',
    version: '0.1.0',
    description: 'Pluggable SMS gateway with provider abstraction and SQLite logging',
    dependencies: [],
  };

  private config: Required<SmsGatewayConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private providers: Map<string, SmsProvider> = new Map();
  private defaultProvider: string | null = null;

  constructor() {
    this.config = {
      dbPath: './data/sms-gateway.db',
      defaultProvider: '',
      maxLogEntries: 10000,
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      dbPath: (config['dbPath'] as string) ?? this.config['dbPath'],
      defaultProvider: (config['defaultProvider'] as string) ?? this.config['defaultProvider'],
      maxLogEntries: (config['maxLogEntries'] as number) ?? this.config['maxLogEntries'],
    };

    this.initDatabase();
    this.defaultProvider = this.config.defaultProvider;
  }

  async start(): Promise<void> {
    this.ctx.logger.info('SmsGateway active', {
      dbPath: this.config.dbPath,
      registeredProviders: this.providers.size,
      defaultProvider: this.defaultProvider,
    });
  }

  async stop(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.ctx.logger.info('SmsGateway stopped');
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const logCount = (
        this.db.prepare('SELECT COUNT(*) as c FROM sms_logs').get() as { c: number }
      ).c;
      const recentErrors = (
        this.db
          .prepare(
            `
        SELECT COUNT(*) as c FROM sms_logs
        WHERE success = 0 AND sent_at > ?
      `,
          )
          .get(Date.now() - 3600000) as { c: number }
      ).c;

      return {
        healthy: true,
        details: {
          totalLogs: logCount,
          errorsLastHour: recentErrors,
          registeredProviders: this.providers.size,
          defaultProvider: this.defaultProvider,
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
   * Register a provider plugin.
   *
   * @param provider - Provider instance implementing SmsProvider interface
   */
  registerProvider(provider: SmsProvider): void {
    if (this.providers.has(provider.name)) {
      this.ctx.logger.warn('Provider already registered, replacing', { provider: provider.name });
    }
    this.providers.set(provider.name, provider);
    this.ctx.logger.info('Provider registered', { provider: provider.name });
  }

  /**
   * Unregister a provider.
   */
  unregisterProvider(name: string): boolean {
    if (this.providers.delete(name)) {
      if (this.defaultProvider === name) {
        this.defaultProvider = null;
      }
      this.ctx.logger.info('Provider unregistered', { provider: name });
      return true;
    }
    return false;
  }

  /**
   * Configure a provider with credentials.
   *
   * @param providerName - Provider to configure
   * @param credentials - API key/secret or other credentials
   */
  configureProvider(providerName: string, credentials: Record<string, string>): void {
    const provider = this.providers.get(providerName);
    if (!provider) {
      throw new Error(`Provider not found: ${providerName}. Register it first.`);
    }
    provider.configure(credentials);
    this.ctx.logger.info('Provider configured', { provider: providerName });
  }

  /**
   * Set the default provider for send operations.
   */
  setDefaultProvider(name: string): boolean {
    if (!this.providers.has(name)) {
      return false;
    }
    this.defaultProvider = name;
    this.ctx.logger.info('Default provider set', { provider: name });
    return true;
  }

  /**
   * Send an SMS via configured provider.
   *
   * @param to - Recipient phone number (E.164 format)
   * @param message - SMS text
   * @param from - Sender ID/phone (optional, provider default if not set)
   * @param providerName - Specific provider to use (default if not specified)
   * @returns SmsResult with status and messageId if successful
   */
  async send(
    to: string,
    message: string,
    from?: string,
    providerName?: string,
  ): Promise<SmsResult> {
    const name = providerName ?? this.defaultProvider;
    if (!name) {
      throw new Error('No provider specified and no default provider set');
    }

    const provider = this.providers.get(name);
    if (!provider) {
      throw new Error(`Provider not registered: ${name}`);
    }

    const result: SmsResult = {
      success: false,
      provider: name,
      timestamp: Date.now(),
    };

    try {
      const sendResult = await provider.send(to, message, from);
      result.success = sendResult.success;
      result.messageId = sendResult.messageId;
      if (!sendResult.success) {
        result.error = sendResult.error;
      }
    } catch (err) {
      result.success = false;
      result.error = err instanceof Error ? err.message : String(err);
    }

    // Log the attempt
    this.logSms({
      id: `sms-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      provider: name,
      to,
      from: from ?? '',
      body: message,
      messageId: result.messageId,
      success: result.success,
      error: result.error,
      sent_at: result.timestamp,
    });

    if (!result.success) {
      this.ctx.logger.warn('SMS send failed', { provider: name, to, error: result.error });
    } else {
      this.ctx.logger.info('SMS sent', { provider: name, to, messageId: result.messageId });
    }

    return result;
  }

  /**
   * Get SMS sending history.
   *
   * @param limit - Max entries to return (most recent first)
   * @param provider - Filter by specific provider (optional)
   * @returns Array of SMS logs
   */
  getHistory(limit: number = 100, provider?: string): SmsLog[] {
    let query = 'SELECT * FROM sms_logs';
    const params: (string | number)[] = [];

    if (provider) {
      query += ' WHERE provider = ?';
      params.push(provider);
    }

    query += ' ORDER BY sent_at DESC LIMIT ?';
    params.push(limit);

    const rows = this.db.prepare(query).all(...params) as Array<{
      id: string;
      provider: string;
      to: string;
      from: string;
      body: string;
      message_id: string | null;
      success: number;
      error: string | null;
      sent_at: number;
    }>;

    return rows.map((r) => ({
      id: r.id,
      provider: r.provider,
      to: r.to,
      from: r.from,
      body: r.body,
      messageId: r.message_id ?? undefined,
      success: r.success === 1,
      error: r.error ?? undefined,
      sent_at: r.sent_at,
    }));
  }

  /**
   * Get sending statistics for a provider.
   *
   * @param providerName - Provider to get stats for
   * @param hoursBack - Lookback period in hours (default 24)
   */
  getProviderStats(
    providerName: string,
    hoursBack: number = 24,
  ): { sent: number; delivered: number; failed: number } {
    const since = Date.now() - hoursBack * 3600000;
    const rows = this.db
      .prepare(
        `
      SELECT
        COUNT(*) as total,
        SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END) as delivered,
        SUM(CASE WHEN success = 0 THEN 1 ELSE 0 END) as failed
      FROM sms_logs
      WHERE provider = ? AND sent_at >= ?
    `,
      )
      .get(providerName, since) as { total: number; delivered: number; failed: number };

    return {
      sent: rows.total,
      delivered: rows.delivered,
      failed: rows.failed,
    };
  }

  /**
   * List registered providers.
   */
  listProviders(): string[] {
    return Array.from(this.providers.keys());
  }

  /** Log SMS to database */
  private logSms(log: Omit<SmsLog, 'id'> & { id: string }): void {
    const stmt = this.db.prepare(`
      INSERT INTO sms_logs (id, provider, to_addr, from_addr, body, message_id, success, error, sent_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
    stmt.run(
      log.id,
      log.provider,
      log.to,
      log.from,
      log.body,
      log.messageId ?? null,
      log.success ? 1 : 0,
      log.error ?? null,
      log.sent_at,
    );

    // Optional cleanup of old logs
    if (this.config.maxLogEntries > 0) {
      const count = (this.db.prepare('SELECT COUNT(*) as c FROM sms_logs').get() as { c: number })
        .c;
      if (count > this.config.maxLogEntries) {
        const deleteCount = count - this.config.maxLogEntries;
        this.db
          .prepare(
            'DELETE FROM sms_logs WHERE id IN (SELECT id FROM sms_logs ORDER BY sent_at ASC LIMIT ?)',
          )
          .run(deleteCount);
      }
    }
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
      CREATE TABLE IF NOT EXISTS sms_logs (
        id TEXT PRIMARY KEY,
        provider TEXT NOT NULL,
        to_addr TEXT NOT NULL,
        from_addr TEXT NOT NULL,
        body TEXT NOT NULL,
        message_id TEXT,
        success INTEGER NOT NULL DEFAULT 0,
        error TEXT,
        sent_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_sms_provider ON sms_logs(provider);
      CREATE INDEX IF NOT EXISTS idx_sms_sent_at ON sms_logs(sent_at DESC);
      CREATE INDEX IF NOT EXISTS idx_sms_success ON sms_logs(success);
    `);
  }
}

// Export the singleton instance
export default new SmsGatewayFeature();

// Also export provider interface for external plugin authors
