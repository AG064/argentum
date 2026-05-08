"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const crypto_1 = __importDefault(require("crypto"));
const path_1 = __importDefault(require("path"));
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
class EncryptedSecretsFeature {
    db;
    masterKey;
    constructor() {
        const dbPath = process.env.AGCLAW_DB_PATH ?? path_1.default.join(process.cwd(), 'data', 'agclaw.db');
        this.db = new better_sqlite3_1.default(dbPath);
        const mk = process.env.AGCLAW_MASTER_KEY;
        if (!mk) {
            throw new Error('AGCLAW_MASTER_KEY not set');
        }
        this.masterKey = Buffer.from(mk, 'hex');
        if (this.masterKey.length !== 32) {
            throw new Error('AGCLAW_MASTER_KEY must be 32 bytes (hex)');
        }
        this.init();
    }
    init() {
        this.db
            .prepare(`
      CREATE TABLE IF NOT EXISTS secrets (
        key TEXT PRIMARY KEY,
        iv TEXT NOT NULL,
        authTag TEXT NOT NULL,
        encryptedValue TEXT NOT NULL
      );
    `)
            .run();
    }
    start() { }
    stop() {
        this.db.close();
    }
    healthCheck() {
        return { ok: true };
    }
    store(key, value) {
        const iv = crypto_1.default.randomBytes(12);
        const cipher = crypto_1.default.createCipheriv('aes-256-gcm', this.masterKey, iv);
        const encrypted = Buffer.concat([cipher.update(value, 'utf8'), cipher.final()]);
        const authTag = cipher.getAuthTag();
        const stmt = this.db.prepare('INSERT OR REPLACE INTO secrets (key, iv, authTag, encryptedValue) VALUES (?, ?, ?, ?)');
        stmt.run(key, iv.toString('hex'), authTag.toString('hex'), encrypted.toString('hex'));
        return { key };
    }
    get(key) {
        const row = this.db
            .prepare('SELECT iv, authTag, encryptedValue FROM secrets WHERE key = ?')
            .get(key);
        if (!row)
            return null;
        const iv = Buffer.from(row.iv, 'hex');
        const authTag = Buffer.from(row.authTag, 'hex');
        const encrypted = Buffer.from(row.encryptedValue, 'hex');
        /* nosemgrep: javascript.node-crypto.security.gcm-no-tag-length.gcm-no-tag-length */
        const decipher = crypto_1.default.createDecipheriv('aes-256-gcm', this.masterKey, iv);
        decipher.setAuthTag(authTag);
        const decrypted = Buffer.concat([decipher.update(encrypted), decipher.final()]).toString('utf8');
        return decrypted;
    }
    delete(key) {
        const info = this.db.prepare('DELETE FROM secrets WHERE key = ?').run(key);
        return { changes: info.changes };
    }
    list() {
        return this.db.prepare('SELECT key FROM secrets').all();
    }
}
exports.default = new EncryptedSecretsFeature();
//# sourceMappingURL=index.js.map