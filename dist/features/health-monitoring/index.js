"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const os_1 = require("os");
// ─── Feature ─────────────────────────────────────────────────────────────────
class HealthMonitoringFeature {
    meta = {
        name: 'health-monitoring',
        version: '0.0.1',
        description: 'System health monitoring with metrics collection and alerts',
        dependencies: [],
    };
    config = {
        enabled: false,
        collectionIntervalMs: 30_000,
        diskCheckPath: '/',
        cpuWarningThreshold: 80,
        memoryWarningThreshold: 80,
        diskWarningThreshold: 80,
    };
    ctx;
    metrics = null;
    featureStatuses = [];
    alerts = [];
    collectionTimer = null;
    lastCollection = 0;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        // Initial collection
        await this.collectMetrics();
        // Start periodic collection
        this.collectionTimer = setInterval(() => {
            this.collectMetrics().catch((err) => {
                this.ctx.logger.error('Health metrics collection failed', { error: err });
            });
        }, this.config.collectionIntervalMs);
        this.ctx.logger.info('Health monitoring active', {
            interval: `${this.config.collectionIntervalMs / 1000}s`,
        });
    }
    async stop() {
        if (this.collectionTimer) {
            clearInterval(this.collectionTimer);
            this.collectionTimer = null;
        }
    }
    async healthCheck() {
        const issues = [];
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
        const unhealthyFeatures = this.featureStatuses.filter((f) => !f.healthy);
        if (unhealthyFeatures.length > 0) {
            issues.push(`Unhealthy features: ${unhealthyFeatures.map((f) => f.name).join(', ')}`);
        }
        return {
            healthy: issues.length === 0,
            message: issues.length > 0 ? issues.join('; ') : undefined,
            details: {
                lastCollection: this.lastCollection,
                metricCount: this.metrics ? 1 : 0,
                activeAlerts: this.alerts.filter((a) => a.level !== 'info').length,
            },
        };
    }
    // ─── Public API ───────────────────────────────────────────────────────────
    /** Get latest system metrics */
    async getSystemHealth() {
        return this.metrics;
    }
    /** Get status of all features (requires plugin-loader integration) */
    async getFeatureStatus() {
        return this.featureStatuses;
    }
    /** Set feature statuses from plugin-loader */
    setFeatureStatuses(statuses) {
        this.featureStatuses = statuses;
    }
    /** Get collected metrics */
    async getMetrics() {
        return this.metrics;
    }
    /** Get active alerts */
    async getAlerts(limit) {
        if (limit) {
            return this.alerts.slice(-limit).reverse();
        }
        return [...this.alerts].reverse();
    }
    /** Clear all alerts */
    async clearAlerts() {
        this.alerts = [];
    }
    // ─── Collection ───────────────────────────────────────────────────────────
    async collectMetrics() {
        const now = Date.now();
        this.metrics = {
            cpu: await this.getCPUInfo(),
            memory: this.getMemoryInfo(),
            disk: this.getDiskInfo(),
            uptime: this.getUptime(),
            loadAvg: this.getLoadAverage(),
            platform: (0, os_1.platform)(),
            arch: (0, os_1.arch)(),
            nodeVersion: process.version,
        };
        this.lastCollection = now;
        // Check thresholds and generate alerts
        this.checkThresholds();
    }
    getMemoryInfo() {
        const total = (0, os_1.totalmem)();
        const free = (0, os_1.freemem)();
        const used = total - free;
        return {
            total,
            free,
            used,
            usagePercent: (used / total) * 100,
        };
    }
    getUptime() {
        return Math.floor(process.uptime());
    }
    getLoadAverage() {
        return (0, os_1.cpus)().map((__cpu) => 0); // Placeholder - we get system load avg separately
    }
    getDiskInfo() {
        try {
            const stats = require('fs').statfsSync(this.config.diskCheckPath);
            const total = stats.blocks * stats.bsize;
            const free = stats.bfree * stats.bsize;
            const used = total - free;
            return [
                {
                    mount: this.config.diskCheckPath,
                    total,
                    free,
                    used,
                    usagePercent: (used / total) * 100,
                },
            ];
        }
        catch (err) {
            this.ctx.logger.warn('Failed to get disk info', { error: err });
            return [];
        }
    }
    async getCPUInfo() {
        try {
            // Try to get CPU model
            const cpuModel = (0, os_1.cpus)()[0]?.model || 'Unknown';
            // Get load average
            const loadAvg = require('os').loadavg();
            // Estimate CPU usage (this is simplified)
            // In production, use a proper library like 'os-cpu'
            const usage = this.estimateCPUUsage();
            return {
                model: cpuModel,
                cores: (0, os_1.cpus)().length,
                loadAverage: loadAvg,
                usage,
            };
        }
        catch (err) {
            this.ctx.logger.warn('Failed to get CPU info', { error: err });
            return {
                model: 'Unknown',
                cores: 0,
                loadAverage: [0, 0, 0],
                usage: 0,
            };
        }
    }
    estimateCPUUsage() {
        // Simple heuristic based on load average vs core count
        const loadAvg = require('os').loadavg()[0] || 0;
        const cores = (0, os_1.cpus)().length;
        return Math.min(100, Math.round((loadAvg / cores) * 100));
    }
    // ─── Alerts ───────────────────────────────────────────────────────────────
    checkThresholds() {
        if (!this.metrics)
            return;
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
    addAlert(level, message) {
        // Avoid duplicate alerts (same message within 5 minutes)
        const recent = this.alerts.filter((a) => a.message === message && Date.now() - a.timestamp < 5 * 60 * 1000);
        if (recent.length > 0)
            return;
        const alert = {
            level,
            message,
            timestamp: Date.now(),
            source: 'health-monitoring',
        };
        this.alerts.push(alert);
        this.ctx.logger[level === 'critical' || level === 'warning' ? 'warn' : 'info']('Alert raised', alert);
    }
}
exports.default = new HealthMonitoringFeature();
//# sourceMappingURL=index.js.map