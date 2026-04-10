import Database from 'better-sqlite3';
declare class AllowlistsFeature {
    db: Database.Database;
    constructor();
    init(): void;
    start(): void;
    stop(): void;
    healthCheck(): {
        ok: boolean;
    };
    addRule(pattern: string, type: 'url' | 'command' | 'user', action: 'allow' | 'deny'): {
        id: number | bigint;
    };
    check(item: {
        value: string;
        type: 'url' | 'command' | 'user';
    }): {
        matched: boolean;
        action: any;
        rule: any;
    } | {
        matched: boolean;
        action: string;
        rule?: undefined;
    };
    removeRule(id: number): {
        changes: number;
    };
}
declare const _default: AllowlistsFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map