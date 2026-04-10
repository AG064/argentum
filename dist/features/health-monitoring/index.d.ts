import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface SystemMetrics {
    cpu: CPUInfo;
    memory: MemoryInfo;
    disk: DiskInfo[];
    uptime: number;
    loadAvg: number[];
    platform: string;
    arch: string;
    nodeVersion: string;
}
export interface CPUInfo {
    model: string;
    cores: number;
    loadAverage: number[];
    usage: number;
}
export interface MemoryInfo {
    total: number;
    free: number;
    used: number;
    usagePercent: number;
}
export interface DiskInfo {
    mount: string;
    total: number;
    free: number;
    used: number;
    usagePercent: number;
}
export interface FeatureStatus {
    name: string;
    state: 'active' | 'disabled' | 'error' | 'unloaded';
    healthy: boolean;
    message?: string;
}
export interface Alert {
    level: 'info' | 'warning' | 'critical';
    message: string;
    timestamp: number;
    source: string;
}
export interface HealthMonitoringConfig {
    enabled: boolean;
    collectionIntervalMs: number;
    diskCheckPath: string;
    cpuWarningThreshold: number;
    memoryWarningThreshold: number;
    diskWarningThreshold: number;
}
declare class HealthMonitoringFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private metrics;
    private featureStatuses;
    private alerts;
    private collectionTimer;
    private lastCollection;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Get latest system metrics */
    getSystemHealth(): Promise<SystemMetrics | null>;
    /** Get status of all features (requires plugin-loader integration) */
    getFeatureStatus(): Promise<FeatureStatus[]>;
    /** Set feature statuses from plugin-loader */
    setFeatureStatuses(statuses: FeatureStatus[]): void;
    /** Get collected metrics */
    getMetrics(): Promise<SystemMetrics | null>;
    /** Get active alerts */
    getAlerts(limit?: number): Promise<Alert[]>;
    /** Clear all alerts */
    clearAlerts(): Promise<void>;
    private collectMetrics;
    private getMemoryInfo;
    private getUptime;
    private getLoadAverage;
    private getDiskInfo;
    private getCPUInfo;
    private estimateCPUUsage;
    private checkThresholds;
    private addAlert;
}
declare const _default: HealthMonitoringFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map