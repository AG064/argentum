/**
 * Skills Library Feature (SQLite)
 *
 * Stores skill records with simple versioning using better-sqlite3.
 * Note: code is stored as text only and never executed by this module.
 */
import type { FeatureModule, FeatureMeta, FeatureContext, HealthStatus } from '../../core/plugin-loader';
export interface SkillRecord {
    id: string;
    name: string;
    version: string;
    description: string;
    code: string;
    tags: string[];
    author?: string;
    createdAt: number;
    updatedAt: number;
}
export interface SkillsLibraryConfig {
    enabled: boolean;
    dbPath: string;
    storageDir: string;
}
declare class SkillsLibraryFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    registerSkill(payload: {
        name: string;
        version?: string;
        description?: string;
        code: string;
        tags?: string[];
        author?: string;
    }): SkillRecord;
    getSkill(id: string): SkillRecord | null;
    listSkills(): SkillRecord[];
    searchSkills(query: string): SkillRecord[];
    updateSkill(id: string, patch: Partial<Omit<SkillRecord, 'id' | 'createdAt'>>): SkillRecord | null;
    removeSkill(id: string): boolean;
    private initDatabase;
    private rowToSkill;
    private safeParse;
}
declare const _default: SkillsLibraryFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map