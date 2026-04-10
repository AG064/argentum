/**
 * Graph Memory Module
 *
 * OMEGA Memory integration — graph-based memory connections.
 * Links memories via typed edges for traversal and pathfinding.
 */
import { type MemoryResult } from './semantic';
/** A node in the memory graph */
export interface MemoryNode {
    id: string;
    type: string;
    content: string;
    weight: number;
    depth: number;
    relation?: string;
    metadata: Record<string, unknown>;
}
/** Graph traversal options */
export interface TraverseOptions {
    maxDepth?: number;
    relationTypes?: string[];
    minWeight?: number;
}
/**
 * MemoryGraph — typed edge connections between memories.
 *
 * Supports BFS traversal, path finding, and connected-node queries
 * on top of the semantic memory store.
 */
export declare class MemoryGraph {
    private db;
    constructor(dbPath?: string);
    /** Add an edge between two memories */
    addEdge(sourceId: string, targetId: string, relation: string, weight?: number): Promise<void>;
    /** Remove an edge */
    removeEdge(sourceId: string, targetId: string, relation?: string): Promise<boolean>;
    /** Traverse graph from a starting node (BFS) */
    traverse(startId: string, options?: TraverseOptions): Promise<MemoryNode[]>;
    /** Find a path between two memories (BFS) */
    findPath(sourceId: string, targetId: string, maxDepth?: number): Promise<MemoryNode[]>;
    /** Get all memories directly connected to a given memory */
    getConnected(id: string): Promise<MemoryResult[]>;
    /** Get edges for a specific memory */
    getEdges(id: string): Promise<Array<{
        source_id: string;
        target_id: string;
        relation_type: string;
        weight: number;
    }>>;
    /** Get statistics about the graph */
    getStats(): Promise<{
        nodes: number;
        edges: number;
        avgDegree: number;
        relationTypes: string[];
    }>;
    /** Find memories that share many connections (community detection, simple) */
    findCommunities(minSharedEdges?: number): Promise<Array<{
        memories: string[];
        sharedEdges: number;
    }>>;
    /** Reconstruct path from BFS visited map */
    private reconstructPath;
    /** Get depth of a path from BFS map */
    private getPathDepth;
    /** Safe JSON parse */
    private parseJson;
}
export declare function getMemoryGraph(dbPath?: string): MemoryGraph;
//# sourceMappingURL=graph.d.ts.map