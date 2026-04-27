"use strict";
// @ts-nocheck
/**
 * Argentum Blueprint System
 *
 * YAML/JSON-driven security configuration.
 * Loads and validates blueprints from:
 * - ~/.ag-claw/blueprint.yaml
 * - ~/.ag-claw/blueprint.json
 * - AGCLAW_BLUEPRINT_PATH env var
 * - Programmatic API
 *
 * Blueprint schema:
 *   version: "1.0"
 *   sandbox:
 *     enabled: true
 *     allowedPaths: [...]
 *     deniedPaths: [...]
 *     maxMemory: 512
 *     networkIsolation: true
 *   policies:
 *     - name: "allow-read-home"
 *       resource: "file://~/.ag-claw/**"
 *       action: "read"
 *       effect: "allow"
 *   credentials:
 *     autoRotate: true
 *     ttlSeconds: 1800
 *   approval:
 *     criticalActions:
 *       - "exec://sudo"
 *       - "network://external"
 *       - "file://delete"
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.BlueprintLoader = void 0;
exports.getBlueprintLoader = getBlueprintLoader;
const fs_1 = require("fs");
const os_1 = require("os");
const path_1 = require("path");
const yaml_1 = require("yaml");
const logger_1 = require("../../core/logger");
const policy_engine_1 = require("../policy-engine");
const sandbox_1 = require("../sandbox");
// ─── Constants ────────────────────────────────────────────────────────────────
const DEFAULT_BLUEPRINT_PATHS = [
    '~/.ag-claw/blueprint.yaml',
    '~/.ag-claw/blueprint.yml',
    '~/.ag-claw/blueprint.json',
    '~/.ag-claw/security.yaml',
    '~/.ag-claw/security.yml',
];
// ─── Validation ───────────────────────────────────────────────────────────────
function validateSandbox(sandbox) {
    if (!sandbox || typeof sandbox !== 'object')
        return null;
    const s = sandbox;
    return {
        enabled: s.enabled !== false,
        allowedPaths: Array.isArray(s.allowedPaths)
            ? s.allowedPaths.filter((p) => typeof p === 'string')
            : [],
        deniedPaths: Array.isArray(s.deniedPaths)
            ? s.deniedPaths.filter((p) => typeof p === 'string')
            : [],
        maxMemory: typeof s.maxMemory === 'number' ? s.maxMemory : undefined,
        maxCpu: typeof s.maxCpu === 'number' ? s.maxCpu : undefined,
        networkIsolation: s.networkIsolation !== false,
        allowExec: s.allowExec !== false,
        maxExecutionTimeMs: typeof s.maxExecutionTimeMs === 'number' ? s.maxExecutionTimeMs : undefined,
    };
}
function validatePolicies(policies) {
    if (!Array.isArray(policies))
        return null;
    const validEffects = new Set(['allow', 'deny', 'approve']);
    return policies
        .filter((p) => p !== null && typeof p === 'object')
        .map((p) => ({
        name: String(p.name ?? 'unnamed'),
        resource: String(p.resource ?? '**'),
        action: String(p.action ?? '*'),
        effect: validEffects.has(p.effect)
            ? p.effect
            : 'deny',
        conditions: Array.isArray(p.conditions) ? p.conditions : undefined,
        requiresApproval: Boolean(p.requiresApproval),
        approvalRisk: p.approvalRisk ?? 'medium',
        priority: typeof p.priority === 'number' ? p.priority : 0,
    }));
}
function validateCredentials(creds) {
    if (!creds || typeof creds !== 'object')
        return null;
    const c = creds;
    return {
        autoRotate: c.autoRotate !== false,
        ttlSeconds: typeof c.ttlSeconds === 'number' ? c.ttlSeconds : 1800,
        providers: typeof c.providers === 'object' && c.providers !== null
            ? c.providers
            : undefined,
    };
}
function validateApproval(approval) {
    if (!approval || typeof approval !== 'object')
        return null;
    const a = approval;
    return {
        criticalActions: Array.isArray(a.criticalActions)
            ? a.criticalActions.filter((c) => typeof c === 'string')
            : [],
        autoExpireSeconds: typeof a.autoExpireSeconds === 'number' ? a.autoExpireSeconds : 300,
        notifyChannels: Array.isArray(a.notifyChannels)
            ? a.notifyChannels.filter((c) => typeof c === 'string')
            : undefined,
    };
}
function validateBlueprint(data) {
    if (!data || typeof data !== 'object')
        return null;
    const d = data;
    const blueprint = {
        version: String(d.version ?? '1.0'),
    };
    if (d.sandbox) {
        blueprint.sandbox = validateSandbox(d.sandbox);
    }
    if (d.policies) {
        blueprint.policies = validatePolicies(d.policies) ?? undefined;
    }
    if (d.credentials) {
        blueprint.credentials = validateCredentials(d.credentials) ?? undefined;
    }
    if (d.approval) {
        blueprint.approval = validateApproval(d.approval) ?? undefined;
    }
    if (d.rateLimits) {
        blueprint.rateLimits =
            typeof d.rateLimits === 'object' ? d.rateLimits : undefined;
    }
    if (d.allowedHosts) {
        blueprint.allowedHosts = Array.isArray(d.allowedHosts)
            ? d.allowedHosts.filter((h) => typeof h === 'string')
            : undefined;
    }
    if (d.allowedPaths) {
        blueprint.allowedPaths = Array.isArray(d.allowedPaths)
            ? d.allowedPaths.filter((p) => typeof p === 'string')
            : undefined;
    }
    return blueprint;
}
// ─── Blueprint Loader ─────────────────────────────────────────────────────────
class BlueprintLoader {
    blueprint = null;
    loadedPath = null;
    logger;
    constructor() {
        this.logger = (0, logger_1.createLogger)().child({ feature: 'blueprint-loader' });
    }
    /**
     * Load blueprint from a YAML or JSON file.
     */
    loadFromFile(filePath) {
        const pathsToTry = filePath
            ? [(0, path_1.resolve)(filePath)]
            : DEFAULT_BLUEPRINT_PATHS.map((p) => (0, path_1.resolve)((0, os_1.homedir)(), p.slice(2)));
        for (const tryPath of pathsToTry) {
            if ((0, fs_1.existsSync)(tryPath)) {
                try {
                    const raw = (0, fs_1.readFileSync)(tryPath, 'utf-8');
                    const data = tryPath.endsWith('.json') ? JSON.parse(raw) : (0, yaml_1.parse)(raw);
                    const validated = validateBlueprint(data);
                    if (validated) {
                        this.blueprint = validated;
                        this.loadedPath = tryPath;
                        this.logger.info(`Blueprint loaded from: ${tryPath}`);
                        return this.blueprint;
                    }
                    else {
                        this.logger.warn(`Blueprint validation failed: ${tryPath}`);
                    }
                }
                catch (err) {
                    this.logger.error(`Failed to load blueprint: ${tryPath}`, {
                        error: err instanceof Error ? err.message : String(err),
                    });
                }
            }
        }
        this.logger.info('No blueprint file found, using defaults');
        return null;
    }
    /**
     * Load blueprint from environment variable (YAML or JSON string).
     */
    loadFromEnv() {
        const envValue = process.env.AGCLAW_BLUEPRINT;
        if (!envValue)
            return null;
        try {
            const data = envValue.startsWith('{') ? JSON.parse(envValue) : (0, yaml_1.parse)(envValue);
            const validated = validateBlueprint(data);
            if (validated) {
                this.blueprint = validated;
                this.loadedPath = 'AGCLAW_BLUEPRINT env var';
                this.logger.info('Blueprint loaded from AGCLAW_BLUEPRINT env var');
                return this.blueprint;
            }
        }
        catch (err) {
            this.logger.error('Failed to parse AGCLAW_BLUEPRINT env var', {
                error: err instanceof Error ? err.message : String(err),
            });
        }
        return null;
    }
    /**
     * Set blueprint programmatically.
     */
    set(blueprint) {
        const validated = validateBlueprint(blueprint);
        if (!validated) {
            throw new Error('Invalid blueprint: validation failed');
        }
        this.blueprint = validated;
        this.loadedPath = 'programmatic';
        this.logger.info('Blueprint set programmatically');
    }
    /**
     * Get the current blueprint.
     */
    get() {
        return this.blueprint;
    }
    /**
     * Apply the blueprint to all security components.
     */
    apply() {
        if (!this.blueprint) {
            this.logger.warn('No blueprint to apply');
            return;
        }
        const policyEngine = (0, policy_engine_1.getPolicyEngine)();
        policyEngine.loadBlueprint(this.blueprint);
        if (this.blueprint.sandbox) {
            const sandbox = (0, sandbox_1.getSandboxExecutor)();
            sandbox.updateConfig({
                enabled: this.blueprint.sandbox.enabled,
                allowedPaths: this.blueprint.sandbox.allowedPaths,
                deniedPaths: this.blueprint.sandbox.deniedPaths,
                maxMemoryMb: this.blueprint.sandbox.maxMemory,
                maxCpuPercent: this.blueprint.sandbox.maxCpu,
                networkIsolation: this.blueprint.sandbox.networkIsolation,
                allowExec: this.blueprint.sandbox.allowExec,
                maxExecutionTimeMs: this.blueprint.sandbox.maxExecutionTimeMs ?? 30000,
            });
        }
        this.logger.info('Blueprint applied to all security components', {
            policiesCount: this.blueprint.policies?.length ?? 0,
            sandboxEnabled: this.blueprint.sandbox?.enabled ?? false,
            credentialsAutoRotate: this.blueprint.credentials?.autoRotate ?? false,
        });
    }
    /**
     * Generate a default blueprint YAML.
     */
    static generateDefaultBlueprint() {
        return `# Argentum Security Blueprint
# Version: 1.0
#
# This file configures Argentum's enterprise security settings.
# Copy to ~/.ag-claw/blueprint.yaml and customize.

version: "1.0"

# Sandbox configuration
sandbox:
  enabled: true
  allowedPaths:
    - ~/ag-claw/
    - ~/ag-claw/data/
    - /tmp/ag-claw-sandbox/
  deniedPaths:
    - /etc/
    - /root/
    - /home/*/.ssh/
    - /home/*/.aws/
    - /home/*/.gcp/
    - "*.env"
    - "*.key"
    - "*.pem"
  maxMemory: 512  # MB
  maxCpu: 50      # percent
  networkIsolation: true
  allowExec: true
  maxExecutionTimeMs: 30000

# Security policies
policies:
  # Allow reading from ag-claw directories
  - name: "allow-read-ag-claw"
    resource: "file://~/ag-claw/**"
    action: "read"
    effect: "allow"
    priority: 10

  # Allow writing to data directory
  - name: "allow-write-data"
    resource: "file://~/ag-claw/data/**"
    action: "write"
    effect: "allow"
    priority: 10

  # Deny access to system files
  - name: "deny-system"
    resource: "file:///etc/**"
    action: "*"
    effect: "deny"
    priority: 100

  - name: "deny-ssh-keys"
    resource: "file://**/.ssh/**"
    action: "*"
    effect: "deny"
    priority: 100

  # Require approval for exec:// actions
  - name: "require-approval-exec"
    resource: "exec://**"
    action: "exec"
    effect: "approve"
    requiresApproval: true
    approvalRisk: "high"
    priority: 50

  # Allow safe network requests
  - name: "allow-https"
    resource: "https://**"
    action: "network"
    effect: "allow"
    priority: 1

  # Block private networks
  - name: "deny-private-network"
    resource: "network://**"
    action: "network"
    effect: "deny"
    priority: 90

# Credential management
credentials:
  autoRotate: true
  ttlSeconds: 1800  # 30 minutes

# Approval configuration
approval:
  criticalActions:
    - "exec://sudo"
    - "exec://rm"
    - "network://external"
    - "file://delete"
    - "file:///etc/**"
  autoExpireSeconds: 300  # 5 minutes

# Rate limiting (optional)
rateLimits:
  api_calls:
    windowMs: 60000
    maxRequests: 60

  file_operations:
    windowMs: 60000
    maxRequests: 100
`;
    }
}
exports.BlueprintLoader = BlueprintLoader;
// ─── Singleton ────────────────────────────────────────────────────────────────
let instance = null;
function getBlueprintLoader() {
    if (!instance) {
        instance = new BlueprintLoader();
    }
    return instance;
}
//# sourceMappingURL=index.js.map