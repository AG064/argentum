import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface SecureProfileRecord {
    id: string;
    name?: string;
    email?: string;
    phone?: string;
    address?: string;
    [k: string]: unknown;
}
export interface SecureProfileConfig {
    storagePath?: string;
    envKeyName?: string;
}
declare class SecureProfileFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private key;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    private encrypt;
    private decrypt;
    private loadAll;
    private saveAll;
    get(id: string): Promise<SecureProfileRecord | null>;
    set(id: string, record: SecureProfileRecord): Promise<void>;
    delete(id: string): Promise<boolean>;
}
declare const _default: SecureProfileFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map