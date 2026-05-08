import Database from 'better-sqlite3';
type JsonRecord = Record<string, unknown>;
declare class AuditLogFeature {
    db: Database.Database;
    constructor();
    init(): void;
    start(): void;
    stop(): void;
    healthCheck(): {
        ok: boolean;
    };
    log(action: string, details: JsonRecord | string, actor?: string, ip?: string): {
        timestamp: number;
    };
    logToolCall(tool: string, input: unknown, output: unknown, actor?: string, success?: boolean, meta?: JsonRecord): {
        timestamp: number;
    };
    logDecision(decision: string, reason: string, actor?: string, meta?: JsonRecord): {
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