export type WorkspaceDecisionReason = 'inside-workspace' | 'outside-workspace' | 'invalid-path' | 'workspace-not-configured';
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
export declare function createWorkspaceBoundary(workspaceRoot: string): WorkspaceBoundary;
//# sourceMappingURL=workspace-boundary.d.ts.map