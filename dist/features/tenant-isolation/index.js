"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
class TenantIsolationFeature {
    meta = {
        name: 'tenant-isolation',
        version: '0.1.0',
        description: 'Provide tenant-scoped data isolation, quotas and cross-tenant access control',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: './data/tenants.db',
        defaultQuotaPerTenant: 100000,
    };
    db;
    ctx;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.initDatabase();
    }
    async start() {
        this.ctx.logger.info('Tenant isolation active', {
            defaultQuota: this.config.defaultQuotaPerTenant,
        });
    }
    async stop() {
        this.db?.close();
    }
    async healthCheck() {
        const tenants = this.db.prepare('SELECT COUNT(*) as c FROM tenants').get().c;
        return { healthy: true, details: { tenantCount: tenants } };
    }
    // Create or fetch tenant record
    ensureTenant(tenantId, displayName) {
        const exists = this.db.prepare('SELECT id FROM tenants WHERE id = ?').get(tenantId);
        if (!exists) {
            this.db
                .prepare('INSERT INTO tenants (id, name, quota) VALUES (?, ?, ?)')
                .run(tenantId, displayName || tenantId, this.config.defaultQuotaPerTenant);
            this.ctx.logger.info('Tenant created', { tenantId });
        }
    }
    // Check quota for tenant, consume if allowConsume true
    checkAndConsumeQuota(tenantId, amount = 1, allowConsume = true) {
        const row = this.db.prepare('SELECT quota FROM tenants WHERE id = ?').get(tenantId);
        if (!row)
            return false;
        const quota = row.quota;
        if (quota < amount)
            return false;
        if (allowConsume) {
            this.db.prepare('UPDATE tenants SET quota = quota - ? WHERE id = ?').run(amount, tenantId);
        }
        return true;
    }
    // Scoped query helper: enforce tenant_id column in where clauses
    scopeQuery(table, tenantId, whereClause = '1=1', params = []) {
        // Note: callers must ensure table has tenant_id column
        const sql = `SELECT * FROM ${table} WHERE tenant_id = ? AND (${whereClause})`;
        return this.db.prepare(sql).all(tenantId, ...params);
    }
    // Admin API: set quota
    setTenantQuota(tenantId, quota) {
        this.db.prepare('UPDATE tenants SET quota = ? WHERE id = ?').run(quota, tenantId);
    }
    getTenantInfo(tenantId) {
        return this.db.prepare('SELECT * FROM tenants WHERE id = ?').get(tenantId);
    }
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
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
exports.default = new TenantIsolationFeature();
//# sourceMappingURL=index.js.map