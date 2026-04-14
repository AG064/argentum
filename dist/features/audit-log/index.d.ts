import Database from 'better-sqlite3';
declare class AuditLogFeature {
    db: Database.Database;
    constructor();
    init(): void;
    start(): void;
    stop(): void;
    healthCheck(): {
        ok: boolean;
    };
    log(action: string, details: Record<string, any> | string, actor?: string, ip?: string): {
        timestamp: number;
    };
    logToolCall(tool: string, input: any, output: any, actor?: string, success?: boolean, meta?: any): {
        timestamp: number;
    };
    logDecision(decision: string, reason: string, actor?: string, meta?: any): {
        timestamp: number;
    };
    query(filters?: {
        action?: string;
        actor?: string;
        since?: number;
        until?: number;
    }): unknown[];
    export(start?: number, end?: number): string;
}
declare const _default: AuditLogFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map