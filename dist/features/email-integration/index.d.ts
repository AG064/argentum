/**
 * Email Integration Feature
 *
 * IMAP/SMTP email integration with secure credential storage.
 * Provides structure for receiving and sending emails without real connection.
 * Passwords are encrypted using Node.js crypto.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Email account configuration (encrypted passwords) */
export interface EmailAccount {
    id: string;
    name: string;
    host: string;
    port: number;
    secure: boolean;
    username: string;
    passwordEncrypted: string;
    passwordIV: string;
    enabled: boolean;
    created_at: number;
}
/** Email message (preview) */
export interface EmailMessage {
    id: string;
    accountId: string;
    folder: string;
    subject: string;
    from: string;
    to: string[];
    date: string;
    snippet: string;
    hasAttachments: boolean;
    isRead: boolean;
}
/** Email message full */
export interface EmailMessageFull extends EmailMessage {
    body: string;
    bodyHtml?: string;
    attachments: Array<{
        filename: string;
        contentType: string;
        size: number;
    }>;
}
/** Search result */
export interface EmailSearchResult {
    messageId: string;
    accountId: string;
    folder: string;
    subject: string;
    from: string;
    date: string;
    snippet: string;
}
/** Feature configuration */
export interface EmailIntegrationConfig {
    dbPath?: string;
    encryptionKey?: string;
    maxConnections?: number;
    defaultImapPort?: number;
    defaultSmtpPort?: number;
}
/**
 * EmailIntegration — secure email account management.
 *
 * Provides:
 * - Encrypted credential storage (AES-256-GCM)
 * - Account configuration
 * - Message listing, retrieval, sending, and search (structure only)
 * - No real IMAP/SMTP connections (stub for future implementation)
 */
declare class EmailIntegrationFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private encryptionKey;
    private readonly ALGORITHM;
    private readonly KEY_LENGTH;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Configure a new email account.
     *
     * @param name - Human-readable account name
     * @param host - IMAP/SMTP host
     * @param port - Port (use defaults if 0)
     * @param username - Email username
     * @param password - Plain password (will be encrypted)
     * @param secure - Use TLS/SSL
     * @returns Created account with encrypted password
     */
    configure(name: string, host: string, port: number, username: string, password: string, secure?: boolean): EmailAccount;
    /**
     * List configured accounts.
     *
     * @returns Array of accounts (passwords encrypted, not decrypted)
     */
    listAccounts(): EmailAccount[];
    /**
     * Get a specific account by ID.
     */
    getAccount(id: string): EmailAccount | null;
    /**
     * Enable/disable an account.
     */
    setAccountEnabled(id: string, enabled: boolean): boolean;
    /**
     * Delete an account and its associated messages.
     */
    deleteAccount(id: string): boolean;
    /**
     * List messages from a folder (stub - would connect to IMAP in real implementation).
     *
     * @param accountId - Account ID
     * @param folder - Folder name (INBOX, Sent, etc.)
     * @param limit - Maximum number of messages
     * @returns Array of message previews
     */
    listMessages(accountId: string, folder?: string, limit?: number): EmailMessage[];
    /**
     * Get a specific message (stub).
     */
    getMessage(accountId: string, messageId: string): EmailMessageFull | null;
    /**
     * Send an email (stub - would connect to SMTP in real implementation).
     *
     * @param accountId - Account to send from
     * @param to - Recipient email(s)
     * @param subject - Email subject
     * @param body - Plain text body
     * @param html - Optional HTML body
     * @returns true if "sent" successfully (stub always returns true)
     */
    send(accountId: string, to: string | string[], subject: string, body: string, html?: string): boolean;
    /**
     * Search messages (stub - would use IMAP SEARCH in real implementation).
     *
     * @param accountId - Account to search
     * @param query - Search query (simple text match for now)
     * @param limit - Max results
     * @returns Matching messages
     */
    search(accountId: string, query: string, limit?: number): EmailSearchResult[];
    /**
     * Store a message in cache (for stub implementations to simulate synced messages).
     * Internal use for testing.
     */
    storeMessage(accountId: string, folder: string, msg: Partial<EmailMessageFull> & {
        id: string;
        date: string;
    }): void;
    /** Validate account exists and is enabled */
    private validateAccount;
    /** Encrypt plaintext password */
    private encrypt;
    /** Decrypt password (internal use) */
    private _decrypt;
    /** Generate a random encryption key if none provided */
    private generateKey;
    /** Initialize database and create tables */
    private initDatabase;
}
declare const _default: EmailIntegrationFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map