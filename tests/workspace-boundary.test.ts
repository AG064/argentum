import { mkdirSync, mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join, resolve } from 'path';

import { createWorkspaceBoundary } from '../src/security/workspace-boundary';

describe('workspace boundary policy', () => {
  test('allows paths inside the selected workspace', () => {
    const root = mkdtempSync(join(tmpdir(), 'argentum-workspace-'));

    try {
      const boundary = createWorkspaceBoundary(root);
      const decision = boundary.checkPath(join(root, 'config', 'default.yaml'));

      expect(decision.allowed).toBe(true);
      expect(decision.reason).toBe('inside-workspace');
      expect(decision.workspaceRoot).toBe(resolve(root));
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test('denies sibling prefix paths outside the selected workspace', () => {
    const parent = mkdtempSync(join(tmpdir(), 'argentum-parent-'));
    const root = join(parent, 'workspace');
    const sibling = join(parent, 'workspace-secret');
    mkdirSync(root);
    mkdirSync(sibling);

    try {
      const boundary = createWorkspaceBoundary(root);
      const decision = boundary.checkPath(join(sibling, 'token.txt'));

      expect(decision.allowed).toBe(false);
      expect(decision.reason).toBe('outside-workspace');
    } finally {
      rmSync(parent, { recursive: true, force: true });
    }
  });

  test('rejects empty or null-byte paths', () => {
    const boundary = createWorkspaceBoundary(process.cwd());

    expect(boundary.checkPath('').allowed).toBe(false);
    expect(boundary.checkPath(`file${String.fromCharCode(0)}name`).allowed).toBe(false);
  });

  test('throws with the denial reason when asserted paths leave the workspace', () => {
    const parent = mkdtempSync(join(tmpdir(), 'argentum-assert-'));
    const root = join(parent, 'workspace');
    mkdirSync(root);

    try {
      const boundary = createWorkspaceBoundary(root);

      expect(() => boundary.assertPath(join(parent, 'secret.txt'))).toThrow(
        /outside-workspace/,
      );
    } finally {
      rmSync(parent, { recursive: true, force: true });
    }
  });
});
