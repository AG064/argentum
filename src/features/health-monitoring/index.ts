import { cpus, totalmem, freemem, arch, platform } from 'os';

import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────────

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
  usage: number; // 0-100
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
  cpuWarningThreshold: number;   // %
  memoryWarningThreshold: number; // %
  diskWarningThreshold: number;  // %
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class HealthMonitoringFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'health-monitoring',
    version: '0.1.0',
    description: 'System health monitoring with metrics collection and alerts',
    dependencies: [],
  };

  private config: HealthMonitoringConfig = {
    enabled: false,
    collectionIntervalMs: 30_000,
    diskCheckPath: '/',
    cpuWarningThreshold: 80,
    memoryWarningThreshold: 80,
    diskWarningThreshold: 80,
  };
  private ctx!: FeatureContext;
  private metrics: SystemMetrics | null = null;
  private featureStatuses: FeatureStatus[] = [];
  private alerts: Alert[] = [];
  private collectionTimer: ReturnType<typeof setInterval> | null = null;
  private lastCollection: number = 0;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<HealthMonitoringConfig>) };
  }

  async start(): Promise<void> {
    // Initial collection
    await this.collectMetrics();

    // Start periodic collection
    this.collectionTimer = setInterval(() => {
      this.collectMetrics().catch(err => {
        this.ctx.logger.error('Health metrics collection failed', { error: err });
      });
    }, this.config.collectionIntervalMs);

    this.ctx.logger.info('Health monitoring active', {
      interval: `${this.config.collectionIntervalMs / 1000}s`,
    });
  }

  async stop(): Promise<void> {
    if (this.collectionTimer) {
      clearInterval(this.collectionTimer);
      this.collectionTimer = null;
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    const issues: string[] = [];

    if (!this.metrics) {
      return { healthy: false, message: 'No metrics collected yet' };
    }

    // Check memory
    if (this.metrics.memory.usagePercent > this.config.memoryWarningThreshold) {
      issues.push(`Memory usage high: ${this.metrics.memory.usagePercent.toFixed(1)}%`);
    }

    // Check disk
    for (const disk of this.metrics.disk) {
      if (disk.usagePercent > this.config.diskWarningThreshold) {
        issues.push(`Disk ${disk.mount} usage high: ${disk.usagePercent.toFixed(1)}%`);
      }
    }

    // Check CPU (if available)
    if (this.metrics.cpu.usage > this.config.cpuWarningThreshold) {
      issues.push(`CPU usage high: ${this.metrics.cpu.usage.toFixed(1)}%`);
    }

    // Check feature statuses
    const unhealthyFeatures = this.featureStatuses.filter(f => !f.healthy);
    if (unhealthyFeatures.length > 0) {
      issues.push(`Unhealthy features: ${unhealthyFeatures.map(f => f.name).join(', ')}`);
    }

    return {
      healthy: issues.length === 0,
      message: issues.length > 0 ? issues.join('; ') : undefined,
      details: {
        lastCollection: this.lastCollection,
        metricCount: this.metrics ? 1 : 0,
        activeAlerts: this.alerts.filter(a => a.level !== 'info').length,
      },
    };
  }

  // ─── Public API ───────────────────────────────────────────────────────────

  /** Get latest system metrics */
  async getSystemHealth(): Promise<SystemMetrics | null> {
    return this.metrics;
  }

  /** Get status of all features (requires plugin-loader integration) */
  async getFeatureStatus(): Promise<FeatureStatus[]> {
    return this.featureStatuses;
  }

  /** Set feature statuses from plugin-loader */
  setFeatureStatuses(statuses: FeatureStatus[]): void {
    this.featureStatuses = statuses;
  }

  /** Get collected metrics */
  async getMetrics(): Promise<SystemMetrics | null> {
    return this.metrics;
  }

  /** Get active alerts */
  async getAlerts(limit?: number): Promise<Alert[]> {
    if (limit) {
      return this.alerts.slice(-limit).reverse();
    }
    return [...this.alerts].reverse();
  }

  /** Clear all alerts */
  async clearAlerts(): Promise<void> {
    this.alerts = [];
  }

  // ─── Collection ───────────────────────────────────────────────────────────

  private async collectMetrics(): Promise<void> {
    const now = Date.now();
    this.metrics = {
      cpu: await this.getCPUInfo(),
      memory: this.getMemoryInfo(),
      disk: this.getDiskInfo(),
      uptime: this.getUptime(),
      loadAvg: this.getLoadAverage(),
      platform: platform(),
      arch: arch(),
      nodeVersion: process.version,
    };
    this.lastCollection = now;

    // Check thresholds and generate alerts
    this.checkThresholds();
  }

  private getMemoryInfo(): MemoryInfo {
    const total = totalmem();
    const free = freemem();
    const used = total - free;
    return {
      total,
      free,
      used,
      usagePercent: (used / total) * 100,
    };
  }

  private getUptime(): number {
    return Math.floor(process.uptime());
  }

  private getLoadAverage(): number[] {
    return cpus().map(__cpu => 0); // Placeholder - we get system load avg separately
  }

  private getDiskInfo(): DiskInfo[] {
    try {
      const stats = require('fs').statfsSync(this.config.diskCheckPath);
      const total = stats.blocks * stats.bsize;
      const free = stats.bfree * stats.bsize;
      const used = total - free;
      return [{
        mount: this.config.diskCheckPath,
        total,
        free,
        used,
        usagePercent: (used / total) * 100,
      }];
    } catch (err) {
      this.ctx.logger.warn('Failed to get disk info', { error: err });
      return [];
    }
  }

  private async getCPUInfo(): Promise<CPUInfo> {
    try {
      // Try to get CPU model
      const cpuModel = cpus()[0]?.model || 'Unknown';

      // Get load average
      const loadAvg = require('os').loadavg() as number[];

      // Estimate CPU usage (this is simplified)
      // In production, use a proper library like 'os-cpu'
      const usage = this.estimateCPUUsage();

      return {
        model: cpuModel,
        cores: cpus().length,
        loadAverage: loadAvg,
        usage,
      };
    } catch (err) {
      this.ctx.logger.warn('Failed to get CPU info', { error: err });
      return {
        model: 'Unknown',
        cores: 0,
        loadAverage: [0, 0, 0],
        usage: 0,
      };
    }
  }

  private estimateCPUUsage(): number {
    // Simple heuristic based on load average vs core count
    const loadAvg = require('os').loadavg()[0] || 0;
    const cores = cpus().length;
    return Math.min(100, Math.round((loadAvg / cores) * 100));
  }

  // ─── Alerts ───────────────────────────────────────────────────────────────

  private checkThresholds(): void {
    if (!this.metrics) return;

    const _now = Date.now();

    // Memory alert
    if (this.metrics.memory.usagePercent > this.config.memoryWarningThreshold) {
      this.addAlert('warning', `Memory usage at ${this.metrics.memory.usagePercent.toFixed(1)}%`);
    }

    // Disk alert
    for (const disk of this.metrics.disk) {
      if (disk.usagePercent > this.config.diskWarningThreshold) {
        this.addAlert('warning', `Disk ${disk.mount} usage at ${disk.usagePercent.toFixed(1)}%`);
      }
    }

    // CPU alert
    if (this.metrics.cpu.usage > this.config.cpuWarningThreshold) {
      this.addAlert('warning', `CPU usage at ${this.metrics.cpu.usage.toFixed(1)}%`);
    }

    // Unhealthy features
    for (const feature of this.featureStatuses) {
      if (!feature.healthy) {
        this.addAlert('critical', `Feature ${feature.name} is unhealthy: ${feature.message}`);
      }
    }
  }

  private addAlert(level: 'info' | 'warning' | 'critical', message: string): void {
    // Avoid duplicate alerts (same message within 5 minutes)
    const recent = this.alerts
      .filter(a => a.message === message && Date.now() - a.timestamp < 5 * 60 * 1000);
    if (recent.length > 0) return;

    const alert: Alert = {
      level,
      message,
      timestamp: Date.now(),
      source: 'health-monitoring',
    };

    this.alerts.push(alert);
    this.ctx.logger[level === 'critical' || level === 'warning' ? 'warn' : 'info']('Alert raised', alert as any);
  }
}

export default new HealthMonitoringFeature();
