import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface Article {
    id: string;
    title: string;
    content: string;
    tags: string[];
    version: number;
    createdAt: number;
    updatedAt: number;
    createdBy: string;
    current: boolean;
}
export interface ArticleVersion {
    version: number;
    content: string;
    updatedAt: number;
    updatedBy: string;
}
export interface KnowledgeBaseConfig {
    enabled: boolean;
    dbPath: string;
    maxArticles: number;
    maxVersionsPerArticle: number;
}
declare class SharedKnowledgeBaseFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private db;
    private ctx;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Add a new article */
    addArticle(title: string, content: string, tags: string[], createdBy: string): Promise<Article>;
    /** Get article by ID */
    getArticle(id: string): Promise<Article | null>;
    /** Get article versions */
    getArticleVersions(id: string): Promise<ArticleVersion[]>;
    /** Update an article (creates new version) */
    updateArticle(id: string, updatedBy: string, title?: string, content?: string, tags?: string[]): Promise<Article | null>;
    /** Delete an article */
    deleteArticle(id: string): Promise<boolean>;
    /** Full-text search across title, content, tags */
    search(query: string, limit?: number, offset?: number): Promise<Article[]>;
    /** Search by tags */
    searchByTags(tags: string[], matchAll?: boolean, limit?: number): Promise<Article[]>;
    private initDatabase;
    private updateFts;
    private mapArticleRow;
}
declare const _default: SharedKnowledgeBaseFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map