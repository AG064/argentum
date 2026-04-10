import Database from 'better-sqlite3';
declare class EncryptedSecretsFeature {
    db: Database.Database;
    masterKey: Buffer;
    constructor();
    init(): void;
    start(): void;
    stop(): void;
    healthCheck(): {
        ok: boolean;
    };
    store(key: string, value: string): {
        key: string;
    };
    get(key: string): string | null;
    delete(key: string): {
        changes: number;
    };
    list(): unknown[];
}
declare const _default: EncryptedSecretsFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map