/**
 * News Digest Feature
 *
 * RSS/Atom feed aggregation with caching. Supports adding/removing sources
 * and generating a digest of recent articles.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** News digest configuration */
export interface NewsDigestConfig {
    enabled: boolean;
    dbPath: string;
    cacheMinutes: number;
    userAgent: string;
    maxArticlesPerSource: number;
}
/** News article */
export interface Article {
    id: string;
    sourceId: string;
    sourceName?: string;
    title: string;
    link: string;
    content?: string;
    description?: string;
    publishedAt: number;
    fetchedAt: number;
}
/** News source */
export interface NewsSource {
    id: string;
    url: string;
    name?: string;
    addedAt: number;
    lastFetched?: number;
    lastArticleCount: number;
}
/**
 * News Digest feature — RSS/Atom feed aggregation.
 *
 * Fetches and caches articles from configured sources, provides digests
 * with latest articles sorted by publication date.
 */
declare class NewsDigestFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private _active;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    private initDb;
    /** Add a new feed source */
    addSource(url: string, name?: string): string;
    /** Remove a feed source and its articles */
    removeSource(id: string): boolean;
    /** Get all sources */
    getSources(): NewsSource[];
    /** Get a digest of recent articles */
    getDigest(sources?: string[], limit?: number): Promise<Article[]>;
    /** Check if a source needs fetching */
    private needsFetch;
    private fetchSourcesIfNeeded;
    /** Fetch and parse a single source (RSS or Atom) */
    private fetchSource;
    /** Parse RSS 2.0 feed */
    private parseRSS;
    /** Parse Atom 1.0 feed */
    private parseAtom;
    /** Force refresh all sources */
    refreshAll(): Promise<void>;
    /** Get source by ID */
    getSource(id: string): NewsSource | null;
}
declare const _default: NewsDigestFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map