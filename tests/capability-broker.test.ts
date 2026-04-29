import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join, resolve } from 'path';

import { createCapabilityBroker } from '../src/security/capability-broker';

describe('capability broker', () => {
  test('allows workspace file access by default and audits the decision', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'argentum-broker-workspace-'));

    try {
      const broker = createCapabilityBroker({ workspaceRoot: workspace });
      const decision = broker.authorize({
        action: 'file.read',
        resource: 'config/default.yaml',
        requester: 'test-agent',
      });

      expect(decision.allowed).toBe(true);
      expect(decision.reason).toBe('inside-workspace');
      expect(decision.resolvedPath).toBe(resolve(workspace, 'config', 'default.yaml'));
      expect(broker.getAuditEntries()).toEqual([
        expect.objectContaining({
          action: 'file.read',
          decision: 'allow',
          requester: 'test-agent',
          reason: 'inside-workspace',
        }),
      ]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test('denies outside-workspace files unless an active scoped grant exists', () => {
    const parent = mkdtempSync(join(tmpdir(), 'argentum-broker-parent-'));
    const workspace = join(parent, 'workspace');
    const outside = join(parent, 'outside');
    mkdirSync(workspace);
    mkdirSync(outside);
    const outsideFile = join(outside, 'notes.txt');
    writeFileSync(outsideFile, 'outside');

    try {
      const broker = createCapabilityBroker({ workspaceRoot: workspace });

      expect(broker.authorize({ action: 'file.read', resource: outsideFile }).allowed).toBe(false);

      const grant = broker.grant({
        action: 'file.read',
        resource: outsideFile,
        scope: 'one-file',
        expiresInMs: 60_000,
        grantedBy: 'user',
        reason: 'User selected this file',
      });

      const granted = broker.authorize({ action: 'file.read', resource: outsideFile });
      expect(granted.allowed).toBe(true);
      expect(granted.reason).toBe('active-grant');
      expect(granted.grantId).toBe(grant.id);

      expect(broker.revokeGrant(grant.id, 'user')).toBe(true);
      expect(broker.authorize({ action: 'file.read', resource: outsideFile }).allowed).toBe(false);
    } finally {
      rmSync(parent, { recursive: true, force: true });
    }
  });

  test('requires explicit shell grants and redacts secrets in audit entries', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'argentum-broker-shell-'));

    try {
      const broker = createCapabilityBroker({ workspaceRoot: workspace });
      const command = 'deploy --token=super-secret-token';

      const denied = broker.authorize({
        action: 'shell.execute',
        resource: command,
        requester: 'test-agent',
      });
      expect(denied.allowed).toBe(false);
      expect(denied.reason).toBe('no-active-grant');

      const serializedAudit = JSON.stringify(broker.getAuditEntries());
      expect(serializedAudit).not.toContain('super-secret-token');
      expect(serializedAudit).toContain('[REDACTED]');

      broker.grant({
        action: 'shell.execute',
        resource: command,
        scope: 'one-command',
        expiresInMs: 60_000,
        grantedBy: 'user',
        reason: 'User approved deployment command',
      });

      expect(broker.authorize({ action: 'shell.execute', resource: command }).allowed).toBe(true);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test('persists redacted audit entries when an audit path is configured', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'argentum-broker-audit-'));
    const auditPath = join(workspace, 'data', 'audit', 'capabilities.log');

    try {
      const broker = createCapabilityBroker({ workspaceRoot: workspace, auditPath });
      const sensitiveValue = ['raw', 'secret', 'token'].join('-');

      broker.authorize({
        action: 'shell.execute',
        resource: `run-tool token=${sensitiveValue}`,
      });

      expect(existsSync(auditPath)).toBe(true);
      const lines = readFileSync(auditPath, 'utf8').trim().split(/\r?\n/);
      expect(lines).toHaveLength(1);
      expect(lines[0]).not.toContain(sensitiveValue);
      expect(lines[0]).toContain('[REDACTED]');
      expect(JSON.parse(lines[0] ?? '{}')).toEqual(
        expect.objectContaining({
          action: 'shell.execute',
          decision: 'deny',
        }),
      );
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
