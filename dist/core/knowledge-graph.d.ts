/**
 * Knowledge Graph Memory with Bitemporal Versioning
 *
 * Graph-based memory for entities, relations, and observations.
 * Supports bitemporal versioning (valid_time + transaction_time) for full
 * audit trails and time-travel queries.
 *
 * Scope hierarchy: global → project → task
 *   - global:  visible everywhere
 *   - project: visible within a project context
 *   - task:    visible within a specific task
 */
export interface Entity {
    id: string;
    type: string;
    name: string;
    properties: Record<string, unknown>;
    observations: string[];
    createdAt: number;
    updatedAt: number;
}
export interface Relation {
    id: string;
    source: string;
    target: string;
    type: string;
    properties: Record<string, unknown>;
}
export interface BitemporalVersion {
    entityId: string;
    validTime: {
        start: number;
        end?: number;
    };
    transactionTime: {
        start: number;
        end?: number;
    };
    snapshot: Entity;
}
type Scope = 'global' | 'project' | 'task';
export declare class KnowledgeGraphMemory {
    private db;
    private scope;
    private scopeId;
    constructor(dbPath?: string);
    private initSchema;
    setScope(scope: Scope, scopeId?: string): void;
    getScope(): string;
    addEntity(entity: Omit<Entity, 'id' | 'createdAt' | 'updatedAt'>): string;
    addRelation(relation: Omit<Relation, 'id'>): string;
    addObservation(entityId: string, observation: string): void;
    query(entities?: string[], relations?: string[]): Entity[];
    getEntity(id: string): Entity | null;
    findPath(from: string, to: string, maxHops?: number): string[];
    search(text: string): Entity[];
    getVersion(entityId: string, asOf: number): Entity | null;
    getRelation(id: string): Relation | null;
    getRelations(entityId: string): Relation[];
    deleteEntity(id: string): void;
    updateEntity(id: string, updates: Partial<Pick<Entity, 'name' | 'properties' | 'observations'>>): void;
    private recordVersion;
    private rowToEntity;
    private rowToRelation;
    /** Close the database connection */
    close(): void;
    /** Get stats */
    stats(): {
        entities: number;
        relations: number;
        versions: number;
    };
}
export {};
//# sourceMappingURL=knowledge-graph.d.ts.map