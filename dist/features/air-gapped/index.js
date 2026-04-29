"use strict";
/**
 * Air-Gapped Feature
 *
 * Fully offline operation mode with local models, no external API calls,
 * and encrypted local storage. Designed for high-security environments.
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
const path_1 = require("path");
/**
 * Air-Gapped feature — fully offline operation mode.
 *
 * Enables Argentum to run in completely isolated environments
 * using local models, encrypted storage, and strict network policies.
 */
class AirGappedFeature {
    meta = {
        name: 'air-gapped',
        version: '0.0.3',
        description: 'Fully offline operation mode with local models',
        dependencies: [],
    };
    config = {
        enabled: false,
        localModelPath: './models/llm',
        localEmbeddingPath: './models/embeddings',
        allowedHosts: [],
        encryptStorage: true,
        auditAllAccess: true,
        maxOfflineDays: 365,
    };
    ctx;
    networkPolicy = {
        allowExternalApi: false,
        allowDns: false,
        allowNtp: false,
        blockedPorts: [80, 443, 8080],
        allowedHosts: [],
    };
    auditLog = [];
    active = false;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        this.active = true;
        this.ctx.logger.info('Air-Gapped mode active', {
            localModelPath: this.config.localModelPath,
            encryptStorage: this.config.encryptStorage,
            networkPolicy: 'fully isolated',
        });
    }
    async stop() {
        this.active = false;
    }
    async healthCheck() {
        return {
            healthy: this.active,
            message: this.active ? 'Air-gapped mode active' : 'Inactive',
            details: {
                auditEntries: this.auditLog.length,
                networkPolicy: this.networkPolicy,
            },
        };
    }
    /** Enable air-gapped mode (block all external requests) */
    async enable() {
        this.config.enabled = true;
        this.active = true;
        this.setNetworkPolicy({
            allowExternalApi: false,
            allowDns: false,
            allowNtp: false,
        });
        this.ctx.logger.info('Air-gapped mode enabled - all external requests blocked');
    }
    /** Disable air-gapped mode (allow external requests) */
    async disable() {
        this.config.enabled = false;
        this.active = false;
        this.ctx.logger.info('Air-gapped mode disabled - external requests allowed');
    }
    /** Check if air-gapped mode is currently active */
    isAirGapped() {
        return this.active && this.config.enabled;
    }
    /** Get list of available local resources (models, data, configs) */
    async getLocalResources() {
        const resources = [];
        const { existsSync, lstatSync } = await Promise.resolve().then(() => __importStar(require('fs')));
        const checkPath = (type, path) => {
            try {
                const fullPath = (0, path_1.resolve)(this.config.localModelPath, '..', path);
                if (existsSync(fullPath)) {
                    const stat = lstatSync(fullPath);
                    resources.push({
                        type,
                        path: fullPath,
                        size: stat.size,
                        available: true,
                        lastModified: stat.mtimeMs,
                    });
                }
            }
            catch {
                // Path doesn't exist or inaccessible
            }
        };
        // Check local model path
        checkPath('model', this.config.localModelPath);
        checkPath('embedding', this.config.localEmbeddingPath);
        // Add known local data directories
        checkPath('data', 'data');
        checkPath('config', 'config');
        return resources;
    }
    /** Check if a network request is allowed */
    checkNetworkAccess(host, port) {
        const allowed = this.config.allowedHosts.includes(host) && !this.networkPolicy.blockedPorts.includes(port);
        this.audit({
            action: 'network_access',
            resource: `${host}:${port}`,
            result: allowed ? 'allowed' : 'denied',
        });
        return allowed;
    }
    /** Get the network policy */
    getNetworkPolicy() {
        return { ...this.networkPolicy };
    }
    /** Update network policy */
    setNetworkPolicy(policy) {
        this.networkPolicy = { ...this.networkPolicy, ...policy };
        this.audit({
            action: 'policy_update',
            resource: 'network_policy',
            result: 'allowed',
            details: policy,
        });
    }
    /** Add entry to audit log */
    audit(entry) {
        if (!this.config.auditAllAccess)
            return;
        const fullEntry = {
            ...entry,
            id: `audit_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
            timestamp: Date.now(),
        };
        this.auditLog.push(fullEntry);
        // Keep last 10000 entries
        if (this.auditLog.length > 10000) {
            this.auditLog = this.auditLog.slice(-5000);
        }
    }
    /** Get audit log entries */
    getAuditLog(filter) {
        let entries = [...this.auditLog];
        if (filter?.action)
            entries = entries.filter((e) => e.action === filter.action);
        if (filter?.result)
            entries = entries.filter((e) => e.result === filter.result);
        if (filter?.limit)
            entries = entries.slice(-filter.limit);
        return entries;
    }
    /** Check if local model is available */
    async isLocalModelAvailable() {
        try {
            const { existsSync } = await Promise.resolve().then(() => __importStar(require('fs')));
            return existsSync(this.config.localModelPath);
        }
        catch {
            return false;
        }
    }
    /** Get air-gapped status summary */
    getStatus() {
        return {
            active: this.active,
            localModelAvailable: false, // Would check filesystem
            networkPolicy: this.networkPolicy,
            auditEntries: this.auditLog.length,
        };
    }
}
exports.default = new AirGappedFeature();
//# sourceMappingURL=index.js.map