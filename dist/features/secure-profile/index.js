"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const crypto_1 = __importDefault(require("crypto"));
const fs_1 = __importDefault(require("fs"));
const path_1 = __importDefault(require("path"));
class SecureProfileFeature {
    meta = {
        name: 'secure-profile',
        version: '0.0.5',
        description: 'Encrypted storage for personal profile data (AES-256-GCM)',
        dependencies: [],
    };
    config;
    ctx;
    key = null;
    constructor() {
        this.config = {
            storagePath: './data/secure-profiles.json.enc',
            envKeyName: 'AG_CLAW_MASTER_KEY',
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            storagePath: config['storagePath'] ?? this.config['storagePath'],
            envKeyName: config['envKeyName'] ?? this.config['envKeyName'],
        };
        const envKey = process.env[this.config.envKeyName];
        if (!envKey) {
            this.ctx.logger.warn('Master key not found in environment; secure-profile will be read-only');
            this.key = null;
        }
        else {
            // Expect base64 or hex
            try {
                this.key = Buffer.from(envKey, envKey.match(/^[0-9a-fA-F]+$/) ? 'hex' : 'base64');
                if (this.key.length < 32) {
                    this.ctx.logger.warn('Master key length < 32 bytes; deriving via HKDF');
                    this.key = crypto_1.default.createHash('sha256').update(this.key).digest();
                }
            }
            catch (e) {
                this.ctx.logger.error(`Failed to parse master key: ${String(e)}`);
                this.key = null;
            }
        }
        // Ensure data dir exists
        const dir = path_1.default.dirname(this.config.storagePath);
        if (!fs_1.default.existsSync(dir))
            fs_1.default.mkdirSync(dir, { recursive: true });
    }
    async start() {
        this.ctx.logger.info('SecureProfile started', { storagePath: this.config.storagePath });
    }
    async stop() {
        this.ctx.logger.info('SecureProfile stopped');
    }
    async healthCheck() {
        try {
            const exists = fs_1.default.existsSync(this.config.storagePath);
            return { healthy: true, details: { encryptedFileExists: exists } };
        }
        catch (e) {
            return { healthy: false, message: String(e) };
        }
    }
    encrypt(json) {
        if (!this.key)
            throw new Error('master key missing');
        const iv = crypto_1.default.randomBytes(12);
        const cipher = crypto_1.default.createCipheriv('aes-256-gcm', this.key.slice(0, 32), iv);
        const enc = Buffer.concat([cipher.update(json), cipher.final()]);
        const tag = cipher.getAuthTag();
        return Buffer.concat([iv, tag, enc]);
    }
    decrypt(blob) {
        if (!this.key)
            throw new Error('master key missing');
        const iv = blob.slice(0, 12);
        const tag = blob.slice(12, 28);
        const enc = blob.slice(28);
        /* nosemgrep: javascript.node-crypto.security.gcm-no-tag-length.gcm-no-tag-length */
        const decipher = crypto_1.default.createDecipheriv('aes-256-gcm', this.key.slice(0, 32), iv);
        /* nosemgrep: javascript.node-crypto.security.gcm-no-tag-length.gcm-no-tag-length */
        decipher.setAuthTag(tag);
        const dec = Buffer.concat([decipher.update(enc), decipher.final()]);
        return dec;
    }
    loadAll() {
        if (!fs_1.default.existsSync(this.config.storagePath))
            return {};
        if (!this.key) {
            this.ctx.logger.warn('Cannot decrypt profiles: master key missing');
            return {};
        }
        try {
            const blob = fs_1.default.readFileSync(this.config.storagePath);
            const json = this.decrypt(blob);
            return JSON.parse(json.toString('utf8'));
        }
        catch (e) {
            this.ctx.logger.error('Failed to load secure profiles');
            return {};
        }
    }
    saveAll(data) {
        if (!this.key)
            throw new Error('master key missing');
        const json = Buffer.from(JSON.stringify(data), 'utf8');
        const blob = this.encrypt(json);
        fs_1.default.writeFileSync(this.config.storagePath, blob, { mode: 0o600 });
    }
    async get(id) {
        const all = this.loadAll();
        return all[id] ?? null;
    }
    async set(id, record) {
        const all = this.loadAll();
        all[id] = { ...record, id };
        this.saveAll(all);
    }
    async delete(id) {
        const all = this.loadAll();
        if (!(id in all))
            return false;
        const { [id]: _removed, ...remaining } = all;
        this.saveAll(remaining);
        return true;
    }
}
exports.default = new SecureProfileFeature();
//# sourceMappingURL=index.js.map