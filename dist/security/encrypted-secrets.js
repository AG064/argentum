"use strict";
/**
 * AG-Claw Encrypted Secrets
 *
 * AES-256-GCM encryption for sensitive values.
 * Master key from AGCLAW_MASTER_KEY env var (or passed explicitly).
 * Secrets stored in a JSON file on disk, encrypted at rest.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.encrypt = encrypt;
exports.decrypt = decrypt;
exports.store = store;
exports.retrieve = retrieve;
exports.removeSecret = removeSecret;
exports.listSecrets = listSecrets;
exports.hasSecret = hasSecret;
exports.setVaultPath = setVaultPath;
exports.clearVaultCache = clearVaultCache;
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const logger_1 = require("../core/logger");
// ─── Constants ────────────────────────────────────────────────
const SALT_LENGTH = 16;
const IV_LENGTH = 16;
const KEY_LENGTH = 32;
const PBKDF2_ITERATIONS = 100_000;
const FILE_VERSION = 1;
const DEFAULT_STORE_PATH = (0, path_1.resolve)(process.cwd(), 'data/secrets.enc.json');
// ─── Logger ───────────────────────────────────────────────────
let logger;
function getLogger() {
    if (!logger) {
        logger = (0, logger_1.createLogger)().child({ feature: 'encrypted-secrets' });
    }
    return logger;
}
// ─── Internal crypto ──────────────────────────────────────────
/**
 * Derive a 256-bit encryption key from the master key + per-secret salt.
 */
function deriveKey(masterKey, salt) {
    return (0, crypto_1.pbkdf2Sync)(masterKey, salt, PBKDF2_ITERATIONS, KEY_LENGTH, 'sha256');
}
/**
 * Resolve the master key buffer.
 * Priority: explicit arg → AGCLAW_MASTER_KEY env → error.
 */
function resolveMasterKey(explicit) {
    const raw = explicit ?? process.env.AGCLAW_MASTER_KEY;
    if (!raw || raw.length === 0) {
        throw new Error('Master key not provided. Set AGCLAW_MASTER_KEY env var or pass key to init().');
    }
    // Derive a stable 32-byte key from whatever passphrase the user provides
    return (0, crypto_1.pbkdf2Sync)(raw, 'ag-claw-master-salt', PBKDF2_ITERATIONS, KEY_LENGTH, 'sha256');
}
// ─── Public API: standalone functions ─────────────────────────
/**
 * Encrypt a plaintext value using AES-256-GCM.
 *
 * Returns a single string: `iv:salt:ciphertext:tag` (all hex).
 *
 * @param masterKey - Master key passphrase or Buffer
 * @param value     - Plaintext to encrypt
 */
function encrypt(masterKey, value) {
    const keyBuf = typeof masterKey === 'string' ? resolveMasterKey(masterKey) : masterKey;
    const iv = (0, crypto_1.randomBytes)(IV_LENGTH);
    const salt = (0, crypto_1.randomBytes)(SALT_LENGTH);
    const derivedKey = deriveKey(keyBuf, salt);
    const cipher = (0, crypto_1.createCipheriv)('aes-256-gcm', derivedKey, iv);
    const encrypted = Buffer.concat([cipher.update(value, 'utf-8'), cipher.final()]);
    const tag = cipher.getAuthTag();
    return [
        iv.toString('hex'),
        salt.toString('hex'),
        encrypted.toString('hex'),
        tag.toString('hex'),
    ].join(':');
}
/**
 * Decrypt a value previously encrypted with encrypt().
 *
 * @param masterKey  - Master key passphrase or Buffer
 * @param encrypted  - String in format `iv:salt:ciphertext:tag`
 */
function decrypt(masterKey, encrypted) {
    const keyBuf = typeof masterKey === 'string' ? resolveMasterKey(masterKey) : masterKey;
    const parts = encrypted.split(':');
    if (parts.length !== 4) {
        throw new Error('Invalid encrypted format. Expected iv:salt:ciphertext:tag');
    }
    const [ivHex, saltHex, ctHex, tagHex] = parts;
    const iv = Buffer.from(ivHex, 'hex');
    const salt = Buffer.from(saltHex, 'hex');
    const derivedKey = deriveKey(keyBuf, salt);
    // nosemgrep: javascript.node-crypto.security.gcm-no-tag-length.gcm-no-tag-length
    // setAuthTag is called below, this is not a vulnerability
    const decipher = (0, crypto_1.createDecipheriv)('aes-256-gcm', derivedKey, iv);
    decipher.setAuthTag(Buffer.from(tagHex, 'hex'));
    const decrypted = Buffer.concat([decipher.update(Buffer.from(ctHex, 'hex')), decipher.final()]);
    return decrypted.toString('utf-8');
}
// ─── Public API: file-backed vault ────────────────────────────
let vaultCache = null;
let vaultPath = DEFAULT_STORE_PATH;
/**
 * Store a secret — encrypts and persists to the vault file.
 *
 * @param key   - Secret name (e.g. "OPENAI_API_KEY")
 * @param value - Secret value
 * @param filePath - Optional vault file path (default: data/secrets.enc.json)
 */
function store(key, value, filePath) {
    const path = filePath ?? vaultPath;
    const masterKey = resolveMasterKey();
    const now = Date.now();
    const vault = loadVault(path);
    const existing = vault.secrets.find((s) => s.key === key);
    // Encrypt with a fresh IV + salt
    const iv = (0, crypto_1.randomBytes)(IV_LENGTH);
    const salt = (0, crypto_1.randomBytes)(SALT_LENGTH);
    const derivedKey = deriveKey(masterKey, salt);
    const cipher = (0, crypto_1.createCipheriv)('aes-256-gcm', derivedKey, iv);
    const encrypted = Buffer.concat([cipher.update(value, 'utf-8'), cipher.final()]);
    const tag = cipher.getAuthTag();
    const entry = {
        key,
        iv: iv.toString('hex'),
        salt: salt.toString('hex'),
        ciphertext: encrypted.toString('hex'),
        tag: tag.toString('hex'),
        createdAt: existing?.createdAt ?? now,
        updatedAt: now,
    };
    if (existing) {
        Object.assign(existing, entry);
    }
    else {
        vault.secrets.push(entry);
    }
    saveVault(path, vault);
    getLogger().info(`Secret stored: ${key}`, { path });
}
/**
 * Retrieve and decrypt a secret from the vault file.
 *
 * @param key      - Secret name
 * @param filePath - Optional vault file path
 * @returns Decrypted value or null if not found
 */
function retrieve(key, filePath) {
    const path = filePath ?? vaultPath;
    const masterKey = resolveMasterKey();
    const vault = loadVault(path);
    const entry = vault.secrets.find((s) => s.key === key);
    if (!entry) {
        getLogger().debug(`Secret not found: ${key}`);
        return null;
    }
    try {
        const derivedKey = deriveKey(masterKey, Buffer.from(entry.salt, 'hex'));
        // nosemgrep: javascript.node-crypto.security.gcm-no-tag-length.gcm-no-tag-length
        // setAuthTag is called below, this is not a vulnerability
        const decipher = (0, crypto_1.createDecipheriv)('aes-256-gcm', derivedKey, Buffer.from(entry.iv, 'hex'));
        decipher.setAuthTag(Buffer.from(entry.tag, 'hex'));
        const decrypted = Buffer.concat([
            decipher.update(Buffer.from(entry.ciphertext, 'hex')),
            decipher.final(),
        ]);
        return decrypted.toString('utf-8');
    }
    catch (err) {
        getLogger().error(`Failed to decrypt secret: ${key}`, {
            error: err instanceof Error ? err.message : String(err),
        });
        return null;
    }
}
/**
 * Delete a secret from the vault.
 */
function removeSecret(key, filePath) {
    const path = filePath ?? vaultPath;
    const vault = loadVault(path);
    const idx = vault.secrets.findIndex((s) => s.key === key);
    if (idx === -1)
        return false;
    vault.secrets.splice(idx, 1);
    saveVault(path, vault);
    getLogger().info(`Secret deleted: ${key}`);
    return true;
}
/**
 * List all secret keys (values are NOT returned).
 */
function listSecrets(filePath) {
    const path = filePath ?? vaultPath;
    const vault = loadVault(path);
    return vault.secrets.map((s) => s.key);
}
/**
 * Check if a secret exists in the vault.
 */
function hasSecret(key, filePath) {
    const path = filePath ?? vaultPath;
    const vault = loadVault(path);
    return vault.secrets.some((s) => s.key === key);
}
/**
 * Set the default vault file path.
 */
function setVaultPath(path) {
    vaultPath = (0, path_1.resolve)(path);
}
// ─── Vault file I/O ───────────────────────────────────────────
function loadVault(path) {
    const resolved = (0, path_1.resolve)(path);
    if (vaultCache)
        return vaultCache;
    if (!(0, fs_1.existsSync)(resolved)) {
        vaultCache = { version: FILE_VERSION, secrets: [] };
        return vaultCache;
    }
    try {
        const raw = (0, fs_1.readFileSync)(resolved, 'utf-8');
        const parsed = JSON.parse(raw);
        vaultCache =
            parsed.version === FILE_VERSION
                ? parsed
                : { version: FILE_VERSION, secrets: parsed.secrets ?? [] };
    }
    catch (err) {
        getLogger().warn(`Corrupt vault file, starting fresh: ${resolved}`);
        vaultCache = { version: FILE_VERSION, secrets: [] };
    }
    return vaultCache;
}
function saveVault(path, vault) {
    const resolved = (0, path_1.resolve)(path);
    const dir = (0, path_1.dirname)(resolved);
    if (!(0, fs_1.existsSync)(dir)) {
        (0, fs_1.mkdirSync)(dir, { recursive: true });
    }
    (0, fs_1.writeFileSync)(resolved, JSON.stringify(vault, null, 2), 'utf-8');
    vaultCache = vault;
}
/**
 * Clear vault cache (for testing or after key rotation).
 */
function clearVaultCache() {
    vaultCache = null;
}
//# sourceMappingURL=encrypted-secrets.js.map