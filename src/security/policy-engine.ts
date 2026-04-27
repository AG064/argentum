/**
 * Argentum Policy Engine
 *
 * YAML-based security policies loaded from config/security-policy.yaml.
 * Evaluates actions against rules by role/agent, enforces rate limits
 * per user/session, and logs all decisions to an audit trail.
 */

import { readFileSync, existsSync, appendFileSync, mkdirSync } from 'fs';
import { resolve, dirname } from 'path';

import { parse } from 'yaml';

import { createLogger, type Logger } from '../core/logger';

// ─── Types ────────────────────────────────────────────────────

export interface PolicyCondition {
  field: string;
  operator: 'equals' | 'not_equals' | 'contains' | 'matches' | 'greater_than' | 'less_than' | 'in';
  value: unknown;
}

export interface PolicyRule {
  id: string;
  name: string;
  description?: string;
  action: 'allow' | 'deny' | 'audit' | 'rate_limit';
  priority: number;
  enabled: boolean;
  auditLevel?: 'none' | 'info' | 'warning' | 'error';
  conditions: PolicyCondition[];
}

export interface RateLimitConfig {
  windowMs: number;
  maxRequests: number;
  keyField: string;
}

export interface PolicyEvaluation {
  allowed: boolean;
  action: string;
  matchedRule?: PolicyRule;
  reason: string;
  shouldAudit: boolean;
}

export interface PolicyContext {
  action: string;
  resource: string;
  user?: string;
  agent?: string;
  role?: string;
  channel?: string;
  feature?: string;
  sessionId?: string;
  [key: string]: unknown;
}

export interface AuditEntry {
  timestamp: number;
  isoTime: string;
  context: PolicyContext;
  evaluation: PolicyEvaluation;
}

interface RateLimitState {
  count: number;
  windowStart: number;
}

// ─── Engine ───────────────────────────────────────────────────

export class PolicyEngine {
  private rules: Map<string, PolicyRule> = new Map();
  private rateLimits: Map<string, RateLimitConfig> = new Map();
  private rateLimitState: Map<string, RateLimitState> = new Map();
  private auditLog: AuditEntry[] = [];
  private logger: Logger;
  private auditFilePath: string | null = null;
  private maxAuditLogSize = 10_000;

  constructor() {
    this.logger = createLogger().child({ feature: 'policy-engine' });
  }

  /**
   * Load policies from the YAML config file (config/security-policy.yaml).
   *
   * Expected structure:
   *   rules:
   *     - id: rule_admin
   *       name: Admin Access
   *       action: allow
   *       priority: 100
   *       enabled: true
   *       conditions:
   *         - field: role
   *           operator: equals
   *           value: admin
   *   rateLimits:
   *     api_calls:
   *       windowMs: 60000
   *       maxRequests: 60
   *       keyField: user
   */
  loadFromFile(filePath: string): void {
    const fullPath = resolve(filePath);
    if (!existsSync(fullPath)) {
      this.logger.warn(`Policy file not found: ${fullPath}`);
      return;
    }

    try {
      const raw = readFileSync(fullPath, 'utf-8');
      const config = parse(raw) as {
        rules?: PolicyRule[];
        rateLimits?: Record<string, RateLimitConfig>;
      };

      // Load rules
      if (config.rules && Array.isArray(config.rules)) {
        for (const rule of config.rules) {
          this.addRule(rule);
        }
        this.logger.info(`Loaded ${config.rules.length} policy rules`);
      }

      // Load rate limits
      if (config.rateLimits) {
        for (const [name, limit] of Object.entries(config.rateLimits)) {
          this.rateLimits.set(name, limit);
        }
        this.logger.info(`Loaded ${Object.keys(config.rateLimits).length} rate limit configs`);
      }
    } catch (err) {
      this.logger.error(`Failed to load policy file: ${fullPath}`, {
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  /**
   * Add a policy rule programmatically.
   */
  addRule(rule: PolicyRule): void {
    this.rules.set(rule.id, rule);
  }

  /**
   * Remove a rule by ID.
   */
  removeRule(id: string): boolean {
    return this.rules.delete(id);
  }

  /**
   * Evaluate an action context against all loaded policies.
   *
   * Evaluation order:
   * 1. Rate limits (if user/session specified)
   * 2. Rules sorted by priority (highest first)
   * 3. First matching rule wins
   * 4. Default: allow
   */
  evaluate(context: PolicyContext): PolicyEvaluation {
    // 1. Check rate limits
    if (context.user || context.sessionId) {
      const rateLimitResult = this.checkRateLimits(context);
      if (rateLimitResult) {
        this.recordAudit(context, rateLimitResult);
        return rateLimitResult;
      }
    }

    // 2. Sort rules by priority (highest first)
    const sortedRules = Array.from(this.rules.values())
      .filter((r) => r.enabled)
      .sort((a, b) => b.priority - a.priority);

    // 3. Find first matching rule
    for (const rule of sortedRules) {
      if (this.evaluateConditions(rule.conditions, context)) {
        const evaluation: PolicyEvaluation = {
          allowed: rule.action === 'allow' || rule.action === 'audit',
          action: rule.action,
          matchedRule: rule,
          reason: `Matched rule: ${rule.name}`,
          shouldAudit: (rule.auditLevel ?? 'none') !== 'none',
        };

        this.recordAudit(context, evaluation);
        return evaluation;
      }
    }

    // 4. Default: allow
    const defaultEval: PolicyEvaluation = {
      allowed: true,
      action: 'allow',
      reason: 'No matching rule, default allow',
      shouldAudit: false,
    };

    return defaultEval;
  }

  /**
   * Quick helper: is this action allowed?
   */
  isAllowed(context: PolicyContext): boolean {
    return this.evaluate(context).allowed;
  }

  /**
   * Get the audit log.
   */
  getAuditLog(limit = 100): AuditEntry[] {
    return this.auditLog.slice(-limit);
  }

  /**
   * Clear the in-memory audit log.
   */
  clearAuditLog(): void {
    this.auditLog = [];
  }

  /**
   * Enable file-based audit logging.
   */
  enableAuditFile(filePath: string): void {
    this.auditFilePath = resolve(filePath);
    const dir = dirname(this.auditFilePath);
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }
    this.logger.info(`Audit file logging enabled: ${this.auditFilePath}`);
  }

  /**
   * Get all loaded rules.
   */
  getRules(): PolicyRule[] {
    return Array.from(this.rules.values()).sort((a, b) => b.priority - a.priority);
  }

  /**
   * Enable or disable a rule.
   */
  setRuleEnabled(id: string, enabled: boolean): boolean {
    const rule = this.rules.get(id);
    if (!rule) return false;
    rule.enabled = enabled;
    return true;
  }

  /**
   * Get rate limit status for a key.
   */
  getRateLimitStatus(key: string): { count: number; remaining: number; resetIn: number } | null {
    const state = this.rateLimitState.get(key);
    if (!state) return null;

    // Find applicable limit (check all, use the most restrictive)
    let minRemaining = Infinity;
    let minResetIn = 0;

    for (const [, config] of this.rateLimits) {
      const elapsed = Date.now() - state.windowStart;
      if (elapsed > config.windowMs) {
        return { count: 0, remaining: config.maxRequests, resetIn: 0 };
      }
      const remaining = Math.max(0, config.maxRequests - state.count);
      const resetIn = config.windowMs - elapsed;
      if (remaining < minRemaining) {
        minRemaining = remaining;
        minResetIn = resetIn;
      }
    }

    return { count: state.count, remaining: minRemaining, resetIn: minResetIn };
  }

  // ─── Internal ─────────────────────────────────────────────

  private evaluateConditions(conditions: PolicyCondition[], context: PolicyContext): boolean {
    if (!conditions || conditions.length === 0) return true;

    return conditions.every((cond) => {
      const value = context[cond.field];

      switch (cond.operator) {
        case 'equals':
          return value === cond.value;

        case 'not_equals':
          return value !== cond.value;

        case 'contains':
          return typeof value === 'string' && value.includes(String(cond.value));

        case 'matches':
          if (typeof value !== 'string') return false;
          try {
            return new RegExp(String(cond.value)).test(value);
          } catch {
            return false;
          }

        case 'greater_than':
          return typeof value === 'number' && value > Number(cond.value);

        case 'less_than':
          return typeof value === 'number' && value < Number(cond.value);

        case 'in':
          return Array.isArray(cond.value) && (cond.value as unknown[]).includes(value);

        default:
          return false;
      }
    });
  }

  private checkRateLimits(context: PolicyContext): PolicyEvaluation | null {
    const now = Date.now();

    for (const [name, config] of this.rateLimits) {
      const keyValue = (context[config.keyField] as string) ?? 'anonymous';
      const stateKey = `${name}:${keyValue}`;

      let state = this.rateLimitState.get(stateKey);

      if (!state || now - state.windowStart > config.windowMs) {
        // New window
        state = { count: 1, windowStart: now };
        this.rateLimitState.set(stateKey, state);
        continue;
      }

      state.count++;

      if (state.count > config.maxRequests) {
        const resetIn = config.windowMs - (now - state.windowStart);
        return {
          allowed: false,
          action: 'rate_limit',
          reason: `Rate limit exceeded for ${name}: ${state.count}/${config.maxRequests} (resets in ${Math.ceil(resetIn / 1000)}s)`,
          shouldAudit: true,
        };
      }
    }

    return null;
  }

  private recordAudit(context: PolicyContext, evaluation: PolicyEvaluation): void {
    if (!evaluation.shouldAudit) return;

    const entry: AuditEntry = {
      timestamp: Date.now(),
      isoTime: new Date().toISOString(),
      context: { ...context },
      evaluation: { ...evaluation },
    };

    // In-memory log
    this.auditLog.push(entry);
    if (this.auditLog.length > this.maxAuditLogSize) {
      this.auditLog = this.auditLog.slice(-Math.floor(this.maxAuditLogSize / 2));
    }

    // File-based audit log
    if (this.auditFilePath) {
      try {
        const line = `${JSON.stringify(entry)}\n`;
        appendFileSync(this.auditFilePath, line, 'utf-8');
      } catch (err) {
        this.logger.error('Failed to write audit log', {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }

    // Log based on audit level
    const level = evaluation.matchedRule?.auditLevel ?? 'info';
    const logMsg = `Policy ${evaluation.action}: ${context.action} by ${context.user ?? 'unknown'}`;
    const logData = {
      rule: evaluation.matchedRule?.id,
      reason: evaluation.reason,
      resource: context.resource,
    };

    switch (level) {
      case 'error':
        this.logger.error(logMsg, logData);
        break;
      case 'warning':
        this.logger.warn(logMsg, logData);
        break;
      default:
        this.logger.info(logMsg, logData);
    }
  }
}

// ─── Singleton ────────────────────────────────────────────────

let instance: PolicyEngine | null = null;

/**
 * Get or create the global policy engine.
 */
export function getPolicyEngine(): PolicyEngine {
  if (!instance) {
    instance = new PolicyEngine();
  }
  return instance;
}

/**
 * Reset the singleton (for testing).
 */
export function resetPolicyEngine(): void {
  instance = null;
}
