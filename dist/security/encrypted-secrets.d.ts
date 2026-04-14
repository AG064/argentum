/**
 * AG-Claw Encrypted Secrets
 *
 * AES-256-GCM encryption for sensitive values.
 * Master key from AGCLAW_MASTER_KEY env var (or passed explicitly).
 * Secrets stored in a JSON file on disk, encrypted at rest.
 */
/**
 * Encrypt a plaintext value using AES-256-GCM.
 *
 * Returns a single string: `iv:salt:ciphertext:tag` (all hex).
 *
 * @param masterKey - Master key passphrase or Buffer
 * @param value     - Plaintext to encrypt
 */
export declare function encrypt(masterKey: string | Buffer, value: string): string;
/**
 * Decrypt a value previously encrypted with encrypt().
 *
 * @param masterKey  - Master key passphrase or Buffer
 * @param encrypted  - String in format `iv:salt:ciphertext:tag`
 */
export declare function decrypt(masterKey: string | Buffer, encrypted: string): string;
/**
 * Store a secret — encrypts and persists to the vault file.
 *
 * @param key   - Secret name (e.g. "OPENAI_API_KEY")
 * @param value - Secret value
 * @param filePath - Optional vault file path (default: data/secrets.enc.json)
 */
export declare function store(key: string, value: string, filePath?: string): void;
/**
 * Retrieve and decrypt a secret from the vault file.
 *
 * @param key      - Secret name
 * @param filePath - Optional vault file path
 * @returns Decrypted value or null if not found
 */
export declare function retrieve(key: string, filePath?: string): string | null;
/**
 * Delete a secret from the vault.
 */
export declare function removeSecret(key: string, filePath?: string): boolean;
/**
 * List all secret keys (values are NOT returned).
 */
export declare function listSecrets(filePath?: string): string[];
/**
 * Check if a secret exists in the vault.
 */
export declare function hasSecret(key: string, filePath?: string): boolean;
/**
 * Set the default vault file path.
 */
export declare function setVaultPath(path: string): void;
/**
 * Clear vault cache (for testing or after key rotation).
 */
export declare function clearVaultCache(): void;
//# sourceMappingURL=encrypted-secrets.d.ts.map