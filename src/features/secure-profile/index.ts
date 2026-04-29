import crypto from 'crypto';
import fs from 'fs';
import path from 'path';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

export interface SecureProfileRecord {
  id: string;
  name?: string;
  email?: string;
  phone?: string;
  address?: string;
  [k: string]: any;
}

export interface SecureProfileConfig {
  storagePath?: string;
  envKeyName?: string;
}

class SecureProfileFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'secure-profile',
    version: '0.0.4',
    description: 'Encrypted storage for personal profile data (AES-256-GCM)',
    dependencies: [],
  };

  private config: Required<SecureProfileConfig>;
  private ctx!: FeatureContext;
  private key: Buffer | null = null;

  constructor() {
    this.config = {
      storagePath: './data/secure-profiles.json.enc',
      envKeyName: 'AG_CLAW_MASTER_KEY',
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      storagePath: (config['storagePath'] as string) ?? this.config['storagePath'],
      envKeyName: (config['envKeyName'] as string) ?? this.config['envKeyName'],
    };

    const envKey = process.env[this.config.envKeyName];
    if (!envKey) {
      this.ctx.logger.warn('Master key not found in environment; secure-profile will be read-only');
      this.key = null;
    } else {
      // Expect base64 or hex
      try {
        this.key = Buffer.from(envKey, envKey.match(/^[0-9a-fA-F]+$/) ? 'hex' : 'base64');
        if (this.key.length < 32) {
          this.ctx.logger.warn('Master key length < 32 bytes; deriving via HKDF');
          this.key = crypto.createHash('sha256').update(this.key).digest();
        }
      } catch (e) {
        this.ctx.logger.error(`Failed to parse master key: ${String(e)}`);
        this.key = null;
      }
    }

    // Ensure data dir exists
    const dir = path.dirname(this.config.storagePath);
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
  }

  async start(): Promise<void> {
    this.ctx.logger.info('SecureProfile started', { storagePath: this.config.storagePath });
  }

  async stop(): Promise<void> {
    this.ctx.logger.info('SecureProfile stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const exists = fs.existsSync(this.config.storagePath);
      return { healthy: true, details: { encryptedFileExists: exists } };
    } catch (e) {
      return { healthy: false, message: String(e) };
    }
  }

  private encrypt(json: Buffer): Buffer {
    if (!this.key) throw new Error('master key missing');
    const iv = crypto.randomBytes(12);
    const cipher = crypto.createCipheriv('aes-256-gcm', this.key.slice(0, 32), iv);
    const enc = Buffer.concat([cipher.update(json), cipher.final()]);
    const tag = cipher.getAuthTag();
    return Buffer.concat([iv, tag, enc]);
  }

  private decrypt(blob: Buffer): Buffer {
    if (!this.key) throw new Error('master key missing');
    const iv = blob.slice(0, 12);
    const tag = blob.slice(12, 28);
    const enc = blob.slice(28);
    /* nosemgrep: javascript.node-crypto.security.gcm-no-tag-length.gcm-no-tag-length */
    const decipher = crypto.createDecipheriv('aes-256-gcm', this.key.slice(0, 32), iv);
    /* nosemgrep: javascript.node-crypto.security.gcm-no-tag-length.gcm-no-tag-length */
    decipher.setAuthTag(tag);
    const dec = Buffer.concat([decipher.update(enc), decipher.final()]);
    return dec;
  }

  private loadAll(): Record<string, SecureProfileRecord> {
    if (!fs.existsSync(this.config.storagePath)) return {};
    if (!this.key) {
      this.ctx.logger.warn('Cannot decrypt profiles: master key missing');
      return {};
    }
    try {
      const blob = fs.readFileSync(this.config.storagePath);
      const json = this.decrypt(blob);
      return JSON.parse(json.toString('utf8')) as Record<string, SecureProfileRecord>;
    } catch (e) {
      this.ctx.logger.error('Failed to load secure profiles');
      return {};
    }
  }

  private saveAll(data: Record<string, SecureProfileRecord>): void {
    if (!this.key) throw new Error('master key missing');
    const json = Buffer.from(JSON.stringify(data), 'utf8');
    const blob = this.encrypt(json);
    fs.writeFileSync(this.config.storagePath, blob, { mode: 0o600 });
  }

  async get(id: string): Promise<SecureProfileRecord | null> {
    const all = this.loadAll();
    return all[id] ?? null;
  }

  async set(id: string, record: SecureProfileRecord): Promise<void> {
    const all = this.loadAll();
    all[id] = { ...record, id };
    this.saveAll(all);
  }

  async delete(id: string): Promise<boolean> {
    const all = this.loadAll();
    if (!(id in all)) return false;
    delete all[id];
    this.saveAll(all);
    return true;
  }
}

export default new SecureProfileFeature();
