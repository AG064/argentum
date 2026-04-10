/**
 * Life Domains Feature
 *
 * Structures memory and knowledge by life domains (work, health, finance,
 * relationships, learning, etc.). Enables domain-specific context retrieval
 * and cross-domain insights.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface LifeDomain {
    id: string;
    name: string;
    description: string;
    color: string;
    icon: string;
    parentId: string | null;
    priority: number;
    createdAt: number;
    updatedAt: number;
}
export interface DomainEntry {
    id: string;
    domainId: string;
    type: 'note' | 'goal' | 'insight' | 'decision' | 'resource' | 'habit';
    title: string;
    content: string;
    tags: string[];
    importance: number;
    metadata: Record<string, unknown>;
    createdAt: number;
    updatedAt: number;
}
export interface DomainStats {
    domainId: string;
    domainName: string;
    entryCount: number;
    lastEntryAt: number | null;
    typeBreakdown: Record<string, number>;
    avgImportance: number;
}
export interface CrossDomainInsight {
    domains: string[];
    pattern: string;
    confidence: number;
    relatedEntries: string[];
}
export interface LifeDomainsConfig {
    enabled: boolean;
    dbPath: string;
    defaultDomains: Array<{
        name: string;
        description: string;
        color: string;
        icon: string;
    }>;
    autoClassify: boolean;
    crossDomainAnalysis: boolean;
}
declare class LifeDomainsFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private db;
    private ctx;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Create a new life domain */
    createDomain(name: string, description: string, options?: {
        color?: string;
        icon?: string;
        parentId?: string;
        priority?: number;
    }): LifeDomain;
    /** List all domains */
    listDomains(): LifeDomain[];
    /** Get a domain by ID */
    getDomain(id: string): LifeDomain | null;
    /** Get domain by name */
    getDomainByName(name: string): LifeDomain | null;
    /** Update a domain */
    updateDomain(id: string, updates: Partial<Omit<LifeDomain, 'id' | 'createdAt'>>): LifeDomain | null;
    /** Delete a domain and all its entries */
    deleteDomain(id: string): boolean;
    /** Add an entry to a domain */
    addEntry(domainId: string, type: DomainEntry['type'], title: string, content: string, options?: {
        tags?: string[];
        importance?: number;
        metadata?: Record<string, unknown>;
    }): DomainEntry;
    /** List entries for a domain */
    listEntries(domainId: string, options?: {
        type?: DomainEntry['type'];
        minImportance?: number;
        limit?: number;
    }): DomainEntry[];
    /** Search entries across all domains */
    searchEntries(query: string, options?: {
        domainId?: string;
        type?: DomainEntry['type'];
        limit?: number;
    }): DomainEntry[];
    /** Get an entry by ID */
    getEntry(id: string): DomainEntry | null;
    /** Update an entry */
    updateEntry(id: string, updates: Partial<Omit<DomainEntry, 'id' | 'domainId' | 'createdAt'>>): DomainEntry | null;
    /** Delete an entry */
    deleteEntry(id: string): boolean;
    /** Auto-classify text into a domain */
    classifyText(text: string): Array<{
        domain: string;
        confidence: number;
    }>;
    /** Auto-add entry to best matching domain */
    autoAddEntry(title: string, content: string, type?: DomainEntry['type']): DomainEntry | null;
    /** Get statistics for all domains */
    getDomainStats(): DomainStats[];
    /** Get recent activity across all domains */
    getRecentActivity(limit?: number): Array<DomainEntry & {
        domainName: string;
    }>;
    private initDatabase;
    private seedDefaultDomains;
    private rowToDomain;
    private rowToEntry;
    private parseJson;
    private parseJsonArray;
}
declare const _default: LifeDomainsFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map