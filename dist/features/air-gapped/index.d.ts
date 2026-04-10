/**
 * Air-Gapped Feature
 *
 * Fully offline operation mode with local models, no external API calls,
 * and encrypted local storage. Designed for high-security environments.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Air-gapped configuration */
export interface AirGappedConfig {
    enabled: boolean;
    localModelPath: string;
    localEmbeddingPath: string;
    allowedHosts: string[];
    encryptStorage: boolean;
    auditAllAccess: boolean;
    maxOfflineDays: number;
}
/** Network policy */
export interface NetworkPolicy {
    allowExternalApi: boolean;
    allowDns: boolean;
    allowNtp: boolean;
    blockedPorts: number[];
    allowedHosts: string[];
}
/** Audit log entry */
export interface AuditEntry {
    id: string;
    timestamp: number;
    action: string;
    resource: string;
    result: 'allowed' | 'denied';
    details?: Record<string, unknown>;
}
/** Local resource information */
export interface LocalResource {
    type: 'model' | 'embedding' | 'data' | 'config';
    path: string;
    size: number;
    available: boolean;
    lastModified?: number;
}
/**
 * Air-Gapped feature — fully offline operation mode.
 *
 * Enables AG-Claw to run in completely isolated environments
 * using local models, encrypted storage, and strict network policies.
 */
declare class AirGappedFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private networkPolicy;
    private auditLog;
    private active;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Enable air-gapped mode (block all external requests) */
    enable(): Promise<void>;
    /** Disable air-gapped mode (allow external requests) */
    disable(): Promise<void>;
    /** Check if air-gapped mode is currently active */
    isAirGapped(): boolean;
    /** Get list of available local resources (models, data, configs) */
    getLocalResources(): Promise<LocalResource[]>;
    /** Check if a network request is allowed */
    checkNetworkAccess(host: string, port: number): boolean;
    /** Get the network policy */
    getNetworkPolicy(): NetworkPolicy;
    /** Update network policy */
    setNetworkPolicy(policy: Partial<NetworkPolicy>): void;
    /** Add entry to audit log */
    private audit;
    /** Get audit log entries */
    getAuditLog(filter?: {
        action?: string;
        result?: 'allowed' | 'denied';
        limit?: number;
    }): AuditEntry[];
    /** Check if local model is available */
    isLocalModelAvailable(): Promise<boolean>;
    /** Get air-gapped status summary */
    getStatus(): {
        active: boolean;
        localModelAvailable: boolean;
        networkPolicy: NetworkPolicy;
        auditEntries: number;
    };
}
declare const _default: AirGappedFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map