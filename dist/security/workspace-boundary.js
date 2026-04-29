"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.createWorkspaceBoundary = createWorkspaceBoundary;
const fs_1 = require("fs");
const path_1 = require("path");
function createWorkspaceBoundary(workspaceRoot) {
    const root = workspaceRoot.trim() ? canonicalize(workspaceRoot) : '';
    return {
        workspaceRoot: root,
        checkPath(requestedPath) {
            if (!root) {
                return deny('workspace-not-configured', requestedPath, '', root);
            }
            if (!requestedPath || requestedPath.includes('\0')) {
                return deny('invalid-path', requestedPath, '', root);
            }
            const absoluteRequest = (0, path_1.isAbsolute)(requestedPath) ? requestedPath : (0, path_1.resolve)(root, requestedPath);
            const resolvedPath = canonicalize(absoluteRequest);
            const relativePath = (0, path_1.relative)(root, resolvedPath);
            const allowed = relativePath === '' || (!relativePath.startsWith('..') && !(0, path_1.isAbsolute)(relativePath));
            return {
                allowed,
                reason: allowed ? 'inside-workspace' : 'outside-workspace',
                requestedPath,
                resolvedPath,
                workspaceRoot: root,
            };
        },
        assertPath(requestedPath) {
            const decision = this.checkPath(requestedPath);
            if (!decision.allowed) {
                throw new Error(`Denied ${requestedPath}: ${decision.reason}`);
            }
            return decision.resolvedPath;
        },
    };
}
function deny(reason, requestedPath, resolvedPath, workspaceRoot) {
    return { allowed: false, reason, requestedPath, resolvedPath, workspaceRoot };
}
function canonicalize(inputPath) {
    const resolved = (0, path_1.resolve)(inputPath);
    const existing = nearestExistingPath(resolved);
    if (!existing)
        return resolved;
    const realExisting = fs_1.realpathSync.native(existing);
    if (existing === resolved)
        return realExisting;
    const suffix = (0, path_1.relative)(existing, resolved);
    return suffix ? (0, path_1.resolve)(realExisting, suffix) : realExisting;
}
function nearestExistingPath(inputPath) {
    let cursor = inputPath;
    while (!(0, fs_1.existsSync)(cursor)) {
        const parent = (0, path_1.dirname)(cursor);
        if (parent === cursor)
            return null;
        cursor = parent;
    }
    return cursor;
}
//# sourceMappingURL=workspace-boundary.js.map