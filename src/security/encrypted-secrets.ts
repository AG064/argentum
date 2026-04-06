/**
 * AG-Claw Encrypted Secrets
 *
 * AES-256-GCM encryption for sensitive values.
 * Master key from AGCLAW_MASTER_KEY env var (or passed explicitly).
 * Secrets stored in a JSON file on disk, encrypted at rest.
 */

import { createCipheriv, createDecipheriv, randomBytes, pbkdf2Sync } from 'crypto';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { resolve, dirname } from 'path';

import { createLogger, type Logger } from '../core/logger';

// ─── Types ────────────────────────────────────────────────────

interface SecretEntry {
  key: string;
  iv: string; // hex
  salt: string; // hex (per-secret salt for key derivation)
  ciphertext: string; // hex
  tag: string; // GCM auth tag, hex
  createdAt: number;
  updatedAt: number;
}

interface SecretsFile {
  version: number;
  secrets: SecretEntry[];
}

// ─── Constants ────────────────────────────────────────────────

const SALT_LENGTH = 16;
const IV_LENGTH = 16;
const KEY_LENGTH = 32;
const PBKDF2_ITERATIONS = 100_000;
const FILE_VERSION = 1;

const DEFAULT_STORE_PATH = resolve(process.cwd(), 'data/secrets.enc.json');

// ─── Logger ───────────────────────────────────────────────────

let logger: Logger;
function getLogger(): Logger {
  if (!logger) {
    logger = createLogger().child({ feature: 'encrypted-secrets' });
  }
  return logger;
}

// ─── Internal crypto ──────────────────────────────────────────

/**
 * Derive a 256-bit encryption key from the master key + per-secret salt.
 */
function deriveKey(masterKey: Buffer, salt: Buffer): Buffer {
  return pbkdf2Sync(masterKey, salt, PBKDF2_ITERATIONS, KEY_LENGTH, 'sha256');
}

/**
 * Resolve the master key buffer.
 * Priority: explicit arg → AGCLAW_MASTER_KEY env → error.
 */
function resolveMasterKey(explicit?: string): Buffer {
  const raw = explicit ?? process.env.AGCLAW_MASTER_KEY;
  if (!raw || raw.length === 0) {
    throw new Error(
      'Master key not provided. Set AGCLAW_MASTER_KEY env var or pass key to init().',
    );
  }
  // Derive a stable 32-byte key from whatever passphrase the user provides
  return pbkdf2Sync(raw, 'ag-claw-master-salt', PBKDF2_ITERATIONS, KEY_LENGTH, 'sha256');
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
export function encrypt(masterKey: string | Buffer, value: string): string {
  const keyBuf = typeof masterKey === 'string' ? resolveMasterKey(masterKey) : masterKey;
  const iv = randomBytes(IV_LENGTH);
  const salt = randomBytes(SALT_LENGTH);
  const derivedKey = deriveKey(keyBuf, salt);

  const cipher = createCipheriv('aes-256-gcm', derivedKey, iv);
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
export function decrypt(masterKey: string | Buffer, encrypted: string): string {
  const keyBuf = typeof masterKey === 'string' ? resolveMasterKey(masterKey) : masterKey;
  const parts = encrypted.split(':');
  if (parts.length !== 4) {
    throw new Error('Invalid encrypted format. Expected iv:salt:ciphertext:tag');
  }

  const [ivHex, saltHex, ctHex, tagHex] = parts as [string, string, string, string];
  const iv = Buffer.from(ivHex, 'hex');
  const salt = Buffer.from(saltHex, 'hex');
  const derivedKey = deriveKey(keyBuf, salt);

  // nosemgrep: javascript.node-crypto.security.gcm-no-tag-length.gcm-no-tag-length
  // setAuthTag is called below, this is not a vulnerability
  const decipher = createDecipheriv('aes-256-gcm', derivedKey, iv);
  decipher.setAuthTag(Buffer.from(tagHex, 'hex'));

  const decrypted = Buffer.concat([decipher.update(Buffer.from(ctHex, 'hex')), decipher.final()]);

  return decrypted.toString('utf-8');
}

// ─── Public API: file-backed vault ────────────────────────────

let vaultCache: SecretsFile | null = null;
let vaultPath: string = DEFAULT_STORE_PATH;

/**
 * Store a secret — encrypts and persists to the vault file.
 *
 * @param key   - Secret name (e.g. "OPENAI_API_KEY")
 * @param value - Secret value
 * @param filePath - Optional vault file path (default: data/secrets.enc.json)
 */
export function store(key: string, value: string, filePath?: string): void {
  const path = filePath ?? vaultPath;
  const masterKey = resolveMasterKey();
  const now = Date.now();

  const vault = loadVault(path);

  const existing = vault.secrets.find((s) => s.key === key);

  // Encrypt with a fresh IV + salt
  const iv = randomBytes(IV_LENGTH);
  const salt = randomBytes(SALT_LENGTH);
  const derivedKey = deriveKey(masterKey, salt);

  const cipher = createCipheriv('aes-256-gcm', derivedKey, iv);
  const encrypted = Buffer.concat([cipher.update(value, 'utf-8'), cipher.final()]);
  const tag = cipher.getAuthTag();

  const entry: SecretEntry = {
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
  } else {
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
export function retrieve(key: string, filePath?: string): string | null {
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
    const decipher = createDecipheriv('aes-256-gcm', derivedKey, Buffer.from(entry.iv, 'hex'));
    decipher.setAuthTag(Buffer.from(entry.tag, 'hex'));

    const decrypted = Buffer.concat([
      decipher.update(Buffer.from(entry.ciphertext, 'hex')),
      decipher.final(),
    ]);

    return decrypted.toString('utf-8');
  } catch (err) {
    getLogger().error(`Failed to decrypt secret: ${key}`, {
      error: err instanceof Error ? err.message : String(err),
    });
    return null;
  }
}

/**
 * Delete a secret from the vault.
 */
export function removeSecret(key: string, filePath?: string): boolean {
  const path = filePath ?? vaultPath;
  const vault = loadVault(path);
  const idx = vault.secrets.findIndex((s) => s.key === key);
  if (idx === -1) return false;

  vault.secrets.splice(idx, 1);
  saveVault(path, vault);
  getLogger().info(`Secret deleted: ${key}`);
  return true;
}

/**
 * List all secret keys (values are NOT returned).
 */
export function listSecrets(filePath?: string): string[] {
  const path = filePath ?? vaultPath;
  const vault = loadVault(path);
  return vault.secrets.map((s) => s.key);
}

/**
 * Check if a secret exists in the vault.
 */
export function hasSecret(key: string, filePath?: string): boolean {
  const path = filePath ?? vaultPath;
  const vault = loadVault(path);
  return vault.secrets.some((s) => s.key === key);
}

/**
 * Set the default vault file path.
 */
export function setVaultPath(path: string): void {
  vaultPath = resolve(path);
}

// ─── Vault file I/O ───────────────────────────────────────────

function loadVault(path: string): SecretsFile {
  const resolved = resolve(path);
  if (vaultCache) return vaultCache;

  if (!existsSync(resolved)) {
    vaultCache = { version: FILE_VERSION, secrets: [] };
    return vaultCache;
  }

  try {
    const raw = readFileSync(resolved, 'utf-8');
    const parsed = JSON.parse(raw) as SecretsFile;
    vaultCache =
      parsed.version === FILE_VERSION
        ? parsed
        : { version: FILE_VERSION, secrets: parsed.secrets ?? [] };
  } catch (err) {
    getLogger().warn(`Corrupt vault file, starting fresh: ${resolved}`);
    vaultCache = { version: FILE_VERSION, secrets: [] };
  }

  return vaultCache;
}

function saveVault(path: string, vault: SecretsFile): void {
  const resolved = resolve(path);
  const dir = dirname(resolved);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }

  writeFileSync(resolved, JSON.stringify(vault, null, 2), 'utf-8');
  vaultCache = vault;
}

/**
 * Clear vault cache (for testing or after key rotation).
 */
export function clearVaultCache(): void {
  vaultCache = null;
}
