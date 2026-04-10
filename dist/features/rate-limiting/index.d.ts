/**
 * Rate Limiting Feature
 *
 * Provides sliding-window rate limiting with configurable limits and keys.
 * Can be used by other features via direct import.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Rate limiting configuration */
export interface RateLimitConfig {
    windowMs: number;
    max: number;
}
declare class RateLimitingFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private db;
    private ctx;
    private config;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    private initDb;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    check(key: string, limit?: number, windowMs?: number): {
        allowed: boolean;
        count: number;
    };
    private checkInternal;
    reset(key: string): {
        changes: number;
    };
    getStats(key: string): number[];
}
declare const _default: RateLimitingFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map