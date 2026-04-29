import { existsSync, realpathSync } from 'fs';
import { dirname, isAbsolute, relative, resolve } from 'path';

export type WorkspaceDecisionReason =
  | 'inside-workspace'
  | 'outside-workspace'
  | 'invalid-path'
  | 'workspace-not-configured';

export interface WorkspacePathDecision {
  allowed: boolean;
  reason: WorkspaceDecisionReason;
  requestedPath: string;
  resolvedPath: string;
  workspaceRoot: string;
}

export interface WorkspaceBoundary {
  workspaceRoot: string;
  checkPath(requestedPath: string): WorkspacePathDecision;
  assertPath(requestedPath: string): string;
}

export function createWorkspaceBoundary(workspaceRoot: string): WorkspaceBoundary {
  const root = workspaceRoot.trim() ? canonicalize(workspaceRoot) : '';

  return {
    workspaceRoot: root,
    checkPath(requestedPath: string): WorkspacePathDecision {
      if (!root) {
        return deny('workspace-not-configured', requestedPath, '', root);
      }

      if (!requestedPath || requestedPath.includes('\0')) {
        return deny('invalid-path', requestedPath, '', root);
      }

      const absoluteRequest = isAbsolute(requestedPath) ? requestedPath : resolve(root, requestedPath);
      const resolvedPath = canonicalize(absoluteRequest);
      const relativePath = relative(root, resolvedPath);
      const allowed =
        relativePath === '' || (!relativePath.startsWith('..') && !isAbsolute(relativePath));

      return {
        allowed,
        reason: allowed ? 'inside-workspace' : 'outside-workspace',
        requestedPath,
        resolvedPath,
        workspaceRoot: root,
      };
    },
    assertPath(requestedPath: string): string {
      const decision = this.checkPath(requestedPath);
      if (!decision.allowed) {
        throw new Error(`Denied ${requestedPath}: ${decision.reason}`);
      }

      return decision.resolvedPath;
    },
  };
}

function deny(
  reason: WorkspaceDecisionReason,
  requestedPath: string,
  resolvedPath: string,
  workspaceRoot: string,
): WorkspacePathDecision {
  return { allowed: false, reason, requestedPath, resolvedPath, workspaceRoot };
}

function canonicalize(inputPath: string): string {
  const resolved = resolve(inputPath);
  const existing = nearestExistingPath(resolved);
  if (!existing) return resolved;

  const realExisting = realpathSync.native(existing);
  if (existing === resolved) return realExisting;

  const suffix = relative(existing, resolved);
  return suffix ? resolve(realExisting, suffix) : realExisting;
}

function nearestExistingPath(inputPath: string): string | null {
  let cursor = inputPath;

  while (!existsSync(cursor)) {
    const parent = dirname(cursor);
    if (parent === cursor) return null;
    cursor = parent;
  }

  return cursor;
}
