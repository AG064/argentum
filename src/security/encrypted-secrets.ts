// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2026 AG064
/**
 * Argentum Encrypted Secrets
 *
 * Configurable encryption for sensitive values.
 * Supports AES-256-GCM and ChaCha20-Poly1305.
 * Master key from ARGENTUM_MASTER_KEY env var (or passed explicitly).
 * Secrets stored in a JSON file on disk, encrypted at rest.
 */

import { createCipheriv, createDecipheriv, randomBytes, pbkdf2Sync } from 'crypto';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { resolve, dirname } from 'path';

import { createLogger, type Logger } from '../core/logger';

// ─── Types ────────────────────────────────────────────────────

export type EncryptionAlgorithm = 'aes-256-gcm' | 'chacha20-poly1305';

interface SecretEntry {
  key: string;
  algorithm: EncryptionAlgorithm;
  iv: string; // hex
  salt: string; // hex (per-secret salt for key derivation)
  ciphertext: string; // hex
  tag: string; // auth tag, hex (GCM tag or Poly1305 tag)
  createdAt: number;
  updatedAt: number;
}

interface SecretsFile {
  version: number;
  algorithm: EncryptionAlgorithm;
  secrets: SecretEntry[];
}

// ─── Constants ────────────────────────────────────────────────

const SALT_LENGTH = 16;
const IV_LENGTH = 16;
const KEY_LENGTH = 32;
const CHACHA20_KEY_LENGTH = 32;
const CHACHA20_IV_LENGTH = 12; // 96-bit nonce for ChaCha20
const PBKDF2_ITERATIONS = 100_000;
const FILE_VERSION = 2; // Bump for algorithm field

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
 * Priority: explicit arg → ARGENTUM_MASTER_KEY env → error.
 */
function resolveMasterKey(explicit?: string): Buffer {
  const raw = explicit ?? process.env.ARGENTUM_MASTER_KEY;
  if (!raw || raw.length === 0) {
    throw new Error(
      'Master key not provided. Set ARGENTUM_MASTER_KEY env var or pass key to init().',
    );
  }
  // Derive a stable 32-byte key from whatever passphrase the user provides
  return pbkdf2Sync(raw, 'ag-claw-master-salt', PBKDF2_ITERATIONS, KEY_LENGTH, 'sha256');
}

// ─── Public API: standalone functions ─────────────────────────

/**
 * Encrypt a plaintext value using the specified algorithm.
 *
 * Returns a single string: `algorithm:iv:salt:ciphertext:tag` (all hex).
 *
 * @param masterKey - Master key passphrase or Buffer
 * @param value     - Plaintext to encrypt
 * @param algorithm - Encryption algorithm to use
 */
export function encrypt(
  masterKey: string | Buffer,
  value: string,
  algorithm: EncryptionAlgorithm = 'aes-256-gcm',
): string {
  const keyBuf = typeof masterKey === 'string' ? resolveMasterKey(masterKey) : masterKey;
  const salt = randomBytes(SALT_LENGTH);
  const derivedKey = deriveKey(keyBuf, salt);

  if (algorithm === 'chacha20-poly1305') {
    const iv = randomBytes(CHACHA20_IV_LENGTH);
    const cipher = createCipheriv('chacha20-poly1305', derivedKey, iv);
    const encrypted = Buffer.concat([cipher.update(value, 'utf-8'), cipher.final()]);
    const tag = cipher.getAuthTag();
    return [
      'chacha20-poly1305',
      iv.toString('hex'),
      salt.toString('hex'),
      encrypted.toString('hex'),
      tag.toString('hex'),
    ].join(':');
  }

  // Default: AES-256-GCM
  const iv = randomBytes(IV_LENGTH);
  const cipher = createCipheriv('aes-256-gcm', derivedKey, iv);
  const encrypted = Buffer.concat([cipher.update(value, 'utf-8'), cipher.final()]);
  const tag = cipher.getAuthTag();

  return [
    'aes-256-gcm',
    iv.toString('hex'),
    salt.toString('hex'),
    encrypted.toString('hex'),
    tag.toString('hex'),
  ].join(':');
}

/**
 * Decrypt a value previously encrypted with encrypt().
 * Automatically detects the algorithm from the encrypted string.
 *
 * @param masterKey  - Master key passphrase or Buffer
 * @param encrypted  - String in format `algorithm:iv:salt:ciphertext:tag`
 */
export function decrypt(masterKey: string | Buffer, encrypted: string): string {
  const keyBuf = typeof masterKey === 'string' ? resolveMasterKey(masterKey) : masterKey;
  const parts = encrypted.split(':');
  if (parts.length !== 5) {
    throw new Error('Invalid encrypted format. Expected algorithm:iv:salt:ciphertext:tag');
  }

  const [algorithm, ivHex, saltHex, ctHex, tagHex] = parts as [
    string,
    string,
    string,
    string,
    string,
  ];
  const iv = Buffer.from(ivHex, 'hex');
  const salt = Buffer.from(saltHex, 'hex');
  const derivedKey = deriveKey(keyBuf, salt);

  if (algorithm === 'chacha20-poly1305') {
    const decipher = createDecipheriv('chacha20-poly1305', derivedKey, iv);
    decipher.setAuthTag(Buffer.from(tagHex, 'hex'));
    const decrypted = Buffer.concat([decipher.update(Buffer.from(ctHex, 'hex')), decipher.final()]);
    return decrypted.toString('utf-8');
  }

  // Default: AES-256-GCM
  const decipher = createDecipheriv('aes-256-gcm', derivedKey, iv);
  decipher.setAuthTag(Buffer.from(tagHex, 'hex'));
  const decrypted = Buffer.concat([decipher.update(Buffer.from(ctHex, 'hex')), decipher.final()]);
  return decrypted.toString('utf-8');
}

/**
 * Get the current default encryption algorithm.
 * Can be set via ARGENTUM_ENCRYPTION_ALGORITHM env var.
 */
export function getDefaultAlgorithm(): EncryptionAlgorithm {
  const env = process.env.ARGENTUM_ENCRYPTION_ALGORITHM;
  if (env === 'chacha20-poly1305' || env === 'aes-256-gcm') {
    return env;
  }
  return 'aes-256-gcm';
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
  const algorithm = getDefaultAlgorithm();

  const vault = loadVault(path);
  const existing = vault.secrets.find((s) => s.key === key);

  // Encrypt with a fresh IV + salt using the configured algorithm
  const encrypted = encrypt(masterKey, value, algorithm);
  const parts = encrypted.split(':');
  if (parts.length !== 5) {
    throw new Error('Invalid encrypted format. Expected algorithm:iv:salt:ciphertext:tag');
  }
  const [alg, ivHex, saltHex, ctHex, tagHex] = parts as [string, string, string, string, string];

  const entry: SecretEntry = {
    key,
    algorithm: alg as EncryptionAlgorithm,
    iv: ivHex,
    salt: saltHex,
    ciphertext: ctHex,
    tag: tagHex,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
  };

  if (existing) {
    Object.assign(existing, entry);
  } else {
    vault.secrets.push(entry);
  }

  saveVault(path, vault);
  getLogger().info(`Secret stored: ${key}`, { path, algorithm });
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
    let decrypted: string;

    if (entry.algorithm === 'chacha20-poly1305') {
      const decipher = createDecipheriv(
        'chacha20-poly1305',
        derivedKey,
        Buffer.from(entry.iv, 'hex'),
      );
      decipher.setAuthTag(Buffer.from(entry.tag, 'hex'));
      decrypted = Buffer.concat([
        decipher.update(Buffer.from(entry.ciphertext, 'hex')),
        decipher.final(),
      ]).toString('utf-8');
    } else {
      // AES-256-GCM
      const decipher = createDecipheriv('aes-256-gcm', derivedKey, Buffer.from(entry.iv, 'hex'));
      decipher.setAuthTag(Buffer.from(entry.tag, 'hex'));
      decrypted = Buffer.concat([
        decipher.update(Buffer.from(entry.ciphertext, 'hex')),
        decipher.final(),
      ]).toString('utf-8');
    }

    return decrypted;
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
    vaultCache = { version: FILE_VERSION, algorithm: getDefaultAlgorithm(), secrets: [] };
    return vaultCache;
  }

  try {
    const raw = readFileSync(resolved, 'utf-8');
    const parsed = JSON.parse(raw) as SecretsFile;
    vaultCache = {
      version: parsed.version ?? FILE_VERSION,
      algorithm: parsed.algorithm ?? getDefaultAlgorithm(),
      secrets: parsed.secrets ?? [],
    };
  } catch (err) {
    getLogger().warn(`Corrupt vault file, starting fresh: ${resolved}`);
    vaultCache = { version: FILE_VERSION, algorithm: getDefaultAlgorithm(), secrets: [] };
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
