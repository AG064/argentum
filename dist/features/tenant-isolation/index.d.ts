import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface TenantIsolationConfig {
    enabled: boolean;
    dbPath: string;
    defaultQuotaPerTenant: number;
}
declare class TenantIsolationFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private db;
    private ctx;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    ensureTenant(tenantId: string, displayName?: string): void;
    checkAndConsumeQuota(tenantId: string, amount?: number, allowConsume?: boolean): boolean;
    scopeQuery(table: string, tenantId: string, whereClause?: string, params?: any[]): unknown[];
    setTenantQuota(tenantId: string, quota: number): void;
    getTenantInfo(tenantId: string): unknown;
    private initDatabase;
}
declare const _default: TenantIsolationFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map