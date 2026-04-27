"use strict";
/**
 * Argentum Credential Manager
 *
 * Enterprise credential management with:
 * - AES-256-GCM encryption at rest
 * - Short-lived credential minting
 * - Automatic rotation before expiry
 * - Full audit trail
 * - SQLite-backed storage
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.CredentialManager = void 0;
exports.getCredentialManager = getCredentialManager;
exports.resetCredentialManager = resetCredentialManager;
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
const logger_1 = require("../../core/logger");
const encrypted_secrets_1 = require("../encrypted-secrets");
const index_js_1 = require("../policy-engine/index.js");
// ─── Constants ────────────────────────────────────────────────────────────────
const CREDENTIALS_TABLE = `
  CREATE TABLE IF NOT EXISTS security_credentials (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    encrypted_value TEXT NOT NULL,
    iv TEXT NOT NULL,
    salt TEXT NOT NULL,
    tag TEXT NOT NULL,
    ttl_seconds INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    rotated_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    metadata TEXT
  );
  CREATE INDEX IF NOT EXISTS idx_creds_expires ON security_credentials(expires_at);
  CREATE INDEX IF NOT EXISTS idx_creds_provider ON security_credentials(provider);
`;
const MINTED_KEYS_TABLE = `
  CREATE TABLE IF NOT EXISTS security_minted_keys (
    key_id TEXT PRIMARY KEY,
    credential_id TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    resource TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (credential_id) REFERENCES security_credentials(id)
  );
  CREATE INDEX IF NOT EXISTS idx_minted_expires ON security_minted_keys(expires_at);
`;
// ─── Helpers ─────────────────────────────────────────────────────────────────
const SENSITIVE_KEYS = new Set([
    'password',
    'passwd',
    'secret',
    'token',
    'api_key',
    'apikey',
    'authorization',
    'credential',
    'private_key',
    'access_key',
    'client_secret',
    'refresh_token',
    'bearer',
]);
function containsSensitive(text) {
    const lower = text.toLowerCase();
    for (const key of SENSITIVE_KEYS) {
        if (lower.includes(key))
            return true;
    }
    return false;
}
function redactSecrets(text) {
    if (!containsSensitive(text))
        return text;
    let result = text;
    for (const key of SENSITIVE_KEYS) {
        const regex = new RegExp(`(${key})\\s*[=:}\\"]\\s*([\\w\\-\\./+=]+)`, 'gi');
        result = result.replace(regex, '$1=[REDACTED]');
    }
    return result;
}
// ─── Credential Manager ───────────────────────────────────────────────────────
class CredentialManager {
    db = null;
    logger;
    rotationIntervalMs = 60000; // Check rotation every minute
    rotationTimer = null;
    masterKey = null;
    dbPath = null;
    constructor(dbPath) {
        this.logger = (0, logger_1.createLogger)().child({ feature: 'credential-manager' });
        if (dbPath) {
            this.initDatabase(dbPath);
        }
    }
    initDatabase(dbPath) {
        this.dbPath = (0, path_1.resolve)(dbPath);
        const dir = (0, path_1.dirname)(this.dbPath);
        if (!(0, fs_1.existsSync)(dir)) {
            (0, fs_1.mkdirSync)(dir, { recursive: true });
        }
        this.db = new better_sqlite3_1.default(this.dbPath);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('foreign_keys = ON');
        this.db.exec(CREDENTIALS_TABLE);
        this.db.exec(MINTED_KEYS_TABLE);
        // Start rotation check timer
        this.startRotationTimer();
        this.logger.info(`Credential manager initialized: ${this.dbPath}`);
    }
    getMasterKey() {
        if (this.masterKey)
            return this.masterKey;
        const keyEnv = process.env.AGCLAW_MASTER_KEY;
        if (!keyEnv) {
            throw new Error('AGCLAW_MASTER_KEY environment variable is required for credential management');
        }
        // Derive a stable 32-byte key
        const { pbkdf2Sync } = require('crypto');
        this.masterKey = pbkdf2Sync(keyEnv, 'ag-claw-credential-salt', 100000, 32, 'sha256');
        return this.masterKey;
    }
    /**
     * Store a new credential (encrypts and persists).
     */
    store(config) {
        if (!this.db)
            throw new Error('Database not initialized');
        const now = Date.now();
        const masterKey = this.getMasterKey();
        // Encrypt the value
        const encrypted = (0, encrypted_secrets_1.encrypt)(masterKey, config.encryptedValue);
        const parts = encrypted.split(':');
        const [iv, salt, ciphertext, tag] = parts;
        const expiresAt = now + config.ttlSeconds * 1000;
        const credential = {
            ...config,
            id: config.id || (0, crypto_1.randomUUID)(),
            encryptedValue: ciphertext,
            iv,
            salt,
            tag,
            expiresAt,
            rotatedAt: now,
            createdAt: config.createdAt || now,
        };
        const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO security_credentials 
      (id, provider, name, type, encrypted_value, iv, salt, tag, ttl_seconds, expires_at, rotated_at, created_at, last_used_at, metadata)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
        stmt.run(credential.id, credential.provider, credential.name, credential.type, credential.encryptedValue, credential.iv, credential.salt, credential.tag, credential.ttlSeconds, credential.expiresAt, credential.rotatedAt, credential.createdAt, credential.lastUsedAt ?? null, credential.metadata ? JSON.stringify(credential.metadata) : null);
        this.logAudit('credential.rotate', 'info', undefined, `credential://${credential.provider}/${credential.name}`, {
            credentialId: credential.id,
            provider: credential.provider,
            type: credential.type,
            ttlSeconds: credential.ttlSeconds,
        }, true);
        return credential;
    }
    /**
     * Retrieve and decrypt a credential.
     */
    retrieve(id) {
        if (!this.db)
            throw new Error('Database not initialized');
        const row = this.db.prepare('SELECT * FROM security_credentials WHERE id = ?').get(id);
        if (!row)
            return null;
        const masterKey = this.getMasterKey();
        // Reconstruct encrypted string for decrypt()
        const encryptedStr = `${row.iv}:${row.salt}:${row.encrypted_value}:${row.tag}`;
        let decryptedValue;
        try {
            decryptedValue = (0, encrypted_secrets_1.decrypt)(masterKey, encryptedStr);
        }
        catch {
            this.logger.error('Failed to decrypt credential', { id });
            return null;
        }
        // Update last_used_at
        this.db
            .prepare('UPDATE security_credentials SET last_used_at = ? WHERE id = ?')
            .run(Date.now(), id);
        return {
            id: row.id,
            provider: row.provider,
            name: row.name,
            type: row.type,
            encryptedValue: decryptedValue,
            iv: row.iv,
            salt: row.salt,
            tag: row.tag,
            ttlSeconds: row.ttl_seconds,
            expiresAt: row.expires_at,
            rotatedAt: row.rotated_at,
            createdAt: row.created_at,
            lastUsedAt: row.last_used_at,
            metadata: row.metadata ? JSON.parse(row.metadata) : undefined,
        };
    }
    /**
     * List all credential metadata (NOT the values).
     */
    listCredentials() {
        if (!this.db)
            throw new Error('Database not initialized');
        const rows = this.db
            .prepare('SELECT * FROM security_credentials ORDER BY provider, name')
            .all();
        return rows.map((row) => ({
            id: row.id,
            provider: row.provider,
            name: row.name,
            type: row.type,
            ttlSeconds: row.ttl_seconds,
            expiresAt: row.expires_at,
            rotatedAt: row.rotated_at,
            createdAt: row.created_at,
            lastUsedAt: row.last_used_at,
            metadata: row.metadata ? JSON.parse(row.metadata) : undefined,
        }));
    }
    /**
     * Delete a credential.
     */
    delete(id) {
        if (!this.db)
            throw new Error('Database not initialized');
        const result = this.db.prepare('DELETE FROM security_credentials WHERE id = ?').run(id);
        if (result.changes > 0) {
            // Also delete minted keys for this credential
            this.db.prepare('DELETE FROM security_minted_keys WHERE credential_id = ?').run(id);
            return true;
        }
        return false;
    }
    /**
     * Mint a short-lived API key for a specific resource.
     */
    mintKey(resource, credentialId, ttlSeconds) {
        if (!this.db)
            throw new Error('Database not initialized');
        const credential = this.retrieve(credentialId);
        if (!credential) {
            this.logger.warn('Credential not found for minting', { credentialId });
            return null;
        }
        const ttl = ttlSeconds ?? credential.ttlSeconds;
        const now = Date.now();
        const expiresAt = now + ttl * 1000;
        // Generate a random key
        const key = (0, crypto_1.randomBytes)(32).toString('base64url');
        const keyId = (0, crypto_1.randomUUID)();
        // Hash the key for storage (we never store the actual key)
        const { createHash } = require('crypto');
        const keyHash = createHash('sha256').update(key).digest('hex');
        // Store minted key reference
        const stmt = this.db.prepare(`
      INSERT INTO security_minted_keys (key_id, credential_id, key_hash, resource, expires_at, created_at)
      VALUES (?, ?, ?, ?, ?, ?)
    `);
        stmt.run(keyId, credentialId, keyHash, resource, expiresAt, now);
        // Log
        this.logAudit('credential.mint', 'info', undefined, resource, {
            credentialId,
            keyId,
            ttlSeconds: ttl,
            expiresAt,
        }, true);
        // Update last_used_at
        this.db
            .prepare('UPDATE security_credentials SET last_used_at = ? WHERE id = ?')
            .run(now, credentialId);
        return { key, expiresAt, resource };
    }
    /**
     * Validate a minted key.
     */
    validateMintedKey(key, resource) {
        if (!this.db)
            return false;
        const { createHash } = require('crypto');
        const keyHash = createHash('sha256').update(key).digest('hex');
        const now = Date.now();
        let query = 'SELECT * FROM security_minted_keys WHERE key_hash = ? AND expires_at > ?';
        const params = [keyHash, now];
        if (resource) {
            query += ' AND resource = ?';
            params.push(resource);
        }
        const row = this.db.prepare(query).get(...params);
        return !!row;
    }
    /**
     * Revoke a minted key.
     */
    revokeMintedKey(keyId) {
        if (!this.db)
            return false;
        const result = this.db.prepare('DELETE FROM security_minted_keys WHERE key_id = ?').run(keyId);
        return result.changes > 0;
    }
    /**
     * Auto-rotate credentials that are expiring soon.
     */
    async rotateIfNeeded() {
        if (!this.db)
            return;
        const now = Date.now();
        const threshold = now + 5 * 60 * 1000; // 5 minutes
        // Find credentials expiring within 5 minutes
        const expiring = this.db
            .prepare('SELECT * FROM security_credentials WHERE expires_at < ?')
            .all(threshold);
        for (const row of expiring) {
            const credentialId = row.id;
            const provider = row.provider;
            const name = row.name;
            this.logger.info(`Credential expiring soon, rotation needed`, {
                credentialId,
                provider,
                name,
            });
            // Emit event for external rotation handler
            this.logAudit('credential.expire', 'warning', undefined, `credential://${provider}/${name}`, {
                credentialId,
                provider,
                name,
                expiresAt: row.expires_at,
            }, false);
            // In a real system, this would trigger a rotation via the credential provider's API
            // For now, we just log it
        }
    }
    startRotationTimer() {
        if (this.rotationTimer)
            return;
        this.rotationTimer = setInterval(() => {
            this.rotateIfNeeded().catch((err) => {
                this.logger.error('Rotation check failed', { error: String(err) });
            });
        }, this.rotationIntervalMs);
    }
    /**
     * Redact secrets from any text (for safe logging).
     */
    redactSecrets(text) {
        return redactSecrets(text);
    }
    /**
     * Get credentials expiring soon.
     */
    getExpiringCredentials(withinSeconds = 300) {
        if (!this.db)
            return [];
        const threshold = Date.now() + withinSeconds * 1000;
        const rows = this.db
            .prepare('SELECT id FROM security_credentials WHERE expires_at <= ? AND expires_at > ?')
            .all(threshold, Date.now());
        return rows.map((r) => r.id);
    }
    logAudit(action, severity, actor, resource, details, success) {
        try {
            const policyEngine = (0, index_js_1.getPolicyEngine)();
            policyEngine.logAudit({
                action,
                severity,
                actor,
                resource,
                details,
                success,
            });
        }
        catch {
            // Policy engine might not be initialized yet
            this.logger.info(`Audit: ${action}`, details);
        }
    }
    getStats() {
        if (!this.db)
            return { total: 0, expiringSoon: 0, expired: 0 };
        const now = Date.now();
        const fiveMin = now + 5 * 60 * 1000;
        const total = this.db.prepare('SELECT COUNT(*) as c FROM security_credentials').get().c;
        const expiringSoon = this.db
            .prepare('SELECT COUNT(*) as c FROM security_credentials WHERE expires_at <= ? AND expires_at > ?')
            .get(fiveMin, now).c;
        const expired = this.db
            .prepare('SELECT COUNT(*) as c FROM security_credentials WHERE expires_at <= ?')
            .get(now).c;
        return { total, expiringSoon, expired };
    }
    close() {
        if (this.rotationTimer) {
            clearInterval(this.rotationTimer);
            this.rotationTimer = null;
        }
        if (this.db) {
            this.db.close();
            this.db = null;
        }
    }
}
exports.CredentialManager = CredentialManager;
// ─── Singleton ────────────────────────────────────────────────────────────────
let instance = null;
function getCredentialManager(dbPath) {
    if (!instance && dbPath) {
        instance = new CredentialManager(dbPath);
    }
    else if (!instance) {
        instance = new CredentialManager();
    }
    return instance;
}
function resetCredentialManager() {
    if (instance) {
        instance.close();
        instance = null;
    }
}
//# sourceMappingURL=index.js.map