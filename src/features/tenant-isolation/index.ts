import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';

export interface TenantIsolationConfig {
  enabled: boolean;
  dbPath: string;
  defaultQuotaPerTenant: number; // arbitrary units (tokens/calls)
}

class TenantIsolationFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'tenant-isolation',
    version: '0.1.0',
    description: 'Provide tenant-scoped data isolation, quotas and cross-tenant access control',
    dependencies: [],
  };

  private config: TenantIsolationConfig = {
    enabled: false,
    dbPath: './data/tenants.db',
    defaultQuotaPerTenant: 100000,
  };
  private db!: Database.Database;
  private ctx!: FeatureContext;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<TenantIsolationConfig>) };
    this.initDatabase();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Tenant isolation active', { defaultQuota: this.config.defaultQuotaPerTenant });
  }

  async stop(): Promise<void> {
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    const tenants = (this.db.prepare('SELECT COUNT(*) as c FROM tenants').get() as any).c as number;
    return { healthy: true, details: { tenantCount: tenants } };
  }

  // Create or fetch tenant record
  ensureTenant(tenantId: string, displayName?: string) {
    const exists = this.db.prepare('SELECT id FROM tenants WHERE id = ?').get(tenantId);
    if (!exists) {
      this.db.prepare('INSERT INTO tenants (id, name, quota) VALUES (?, ?, ?)').run(tenantId, displayName || tenantId, this.config.defaultQuotaPerTenant);
      this.ctx.logger.info('Tenant created', { tenantId });
    }
  }

  // Check quota for tenant, consume if allowConsume true
  checkAndConsumeQuota(tenantId: string, amount: number = 1, allowConsume: boolean = true): boolean {
    const row = this.db.prepare('SELECT quota FROM tenants WHERE id = ?').get(tenantId) as any;
    if (!row) return false;
    const quota = row.quota as number;
    if (quota < amount) return false;
    if (allowConsume) {
      this.db.prepare('UPDATE tenants SET quota = quota - ? WHERE id = ?').run(amount, tenantId);
    }
    return true;
  }

  // Scoped query helper: enforce tenant_id column in where clauses
  scopeQuery(table: string, tenantId: string, whereClause: string = '1=1', params: any[] = []) {
    // Note: callers must ensure table has tenant_id column
    const sql = `SELECT * FROM ${table} WHERE tenant_id = ? AND (${whereClause})`;
    return this.db.prepare(sql).all(tenantId, ...params);
  }

  // Admin API: set quota
  setTenantQuota(tenantId: string, quota: number) {
    this.db.prepare('UPDATE tenants SET quota = ? WHERE id = ?').run(quota, tenantId);
  }

  getTenantInfo(tenantId: string) {
    return this.db.prepare('SELECT * FROM tenants WHERE id = ?').get(tenantId);
  }

  private initDatabase() {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS tenants (
        id TEXT PRIMARY KEY,
        name TEXT,
        quota INTEGER NOT NULL DEFAULT ${this.config.defaultQuotaPerTenant}
      );

      -- Example of tenant-scoped data: resources table must include tenant_id
      CREATE TABLE IF NOT EXISTS tenant_resources (
        id TEXT PRIMARY KEY,
        tenant_id TEXT NOT NULL,
        key TEXT,
        value TEXT,
        FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_resources_tenant ON tenant_resources(tenant_id);
    `);
  }
}

export default new TenantIsolationFeature();
