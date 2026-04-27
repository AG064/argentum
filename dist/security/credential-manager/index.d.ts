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
import type { StoredCredential, MintedKey } from '../types';
export declare class CredentialManager {
    private db;
    private logger;
    private rotationIntervalMs;
    private rotationTimer;
    private masterKey;
    private dbPath;
    constructor(dbPath?: string);
    private initDatabase;
    private getMasterKey;
    /**
     * Store a new credential (encrypts and persists).
     */
    store(config: Omit<StoredCredential, 'iv' | 'salt' | 'tag' | 'rotatedAt'>): StoredCredential;
    /**
     * Retrieve and decrypt a credential.
     */
    retrieve(id: string): StoredCredential | null;
    /**
     * List all credential metadata (NOT the values).
     */
    listCredentials(): Omit<StoredCredential, 'encryptedValue' | 'iv' | 'salt' | 'tag'>[];
    /**
     * Delete a credential.
     */
    delete(id: string): boolean;
    /**
     * Mint a short-lived API key for a specific resource.
     */
    mintKey(resource: string, credentialId: string, ttlSeconds?: number): MintedKey | null;
    /**
     * Validate a minted key.
     */
    validateMintedKey(key: string, resource?: string): boolean;
    /**
     * Revoke a minted key.
     */
    revokeMintedKey(keyId: string): boolean;
    /**
     * Auto-rotate credentials that are expiring soon.
     */
    rotateIfNeeded(): Promise<void>;
    private startRotationTimer;
    /**
     * Redact secrets from any text (for safe logging).
     */
    redactSecrets(text: string): string;
    /**
     * Get credentials expiring soon.
     */
    getExpiringCredentials(withinSeconds?: number): StoredCredential['id'][];
    private logAudit;
    getStats(): {
        total: number;
        expiringSoon: number;
        expired: number;
    };
    close(): void;
}
export declare function getCredentialManager(dbPath?: string): CredentialManager;
export declare function resetCredentialManager(): void;
//# sourceMappingURL=index.d.ts.map