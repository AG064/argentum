/**
 * Knowledge Graph Feature
 *
 * Entity-relationship knowledge graph with file import (Markdown, JSON),
 * graph export, BFS pathfinding, and SQLite + in-memory backends.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Knowledge Graph configuration */
export interface KnowledgeGraphConfig {
    enabled: boolean;
    backend: 'sqlite' | 'memory';
    path: string;
    importDir: string;
    exportDir: string;
}
/** Graph entity (node) */
export interface Entity {
    id: string;
    type: string;
    name: string;
    properties: Record<string, unknown>;
    tags: string[];
    createdAt: number;
    updatedAt: number;
}
/** Graph relationship (edge) */
export interface Relationship {
    id: string;
    sourceId: string;
    targetId: string;
    type: string;
    properties: Record<string, unknown>;
    weight: number;
    createdAt: number;
}
/** Query result */
export interface QueryResult {
    entities: Entity[];
    relationships: Relationship[];
    paths?: Entity[][];
}
/** Graph data for export */
export interface GraphData {
    entities: Entity[];
    relationships: Relationship[];
    exportedAt: number;
    stats: {
        entities: number;
        relationships: number;
    };
}
/**
 * Knowledge Graph feature — entity-relationship graph with file import/export.
 */
declare class KnowledgeGraphFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private backend;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    addEntity(type: string, name: string, properties?: Record<string, unknown>, tags?: string[]): Promise<Entity>;
    getEntity(id: string): Promise<Entity | null>;
    updateEntity(id: string, updates: Partial<Entity>): Promise<Entity>;
    deleteEntity(id: string): Promise<void>;
    addRelationship(sourceId: string, targetId: string, type: string, properties?: Record<string, unknown>, weight?: number): Promise<Relationship>;
    findEntities(query: {
        type?: string;
        name?: string;
        tags?: string[];
    }): Promise<Entity[]>;
    findPaths(sourceId: string, targetId: string, maxDepth?: number): Promise<Entity[][]>;
    /**
     * Get graph data formatted for 3D visualization
     * Returns {nodes, links} format compatible with 3d-force-graph
     */
    getGraph3DData(): Promise<{
        entities: Entity[];
        relationships: Relationship[];
    }>;
    /** Import from Markdown file */
    importFromMarkdown(filePath: string): Promise<{
        entities: number;
        relationships: number;
    }>;
    /** Import from JSON file */
    importFromJson(filePath: string): Promise<{
        entities: number;
        relationships: number;
    }>;
    /** Import from any supported file */
    importFromFile(filePath: string): Promise<{
        entities: number;
        relationships: number;
    }>;
    /** Export as JSON graph data */
    exportGraph(): Promise<GraphData>;
    /** Export to JSON file */
    exportToFile(filePath: string): Promise<GraphData>;
}
declare const _default: KnowledgeGraphFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map