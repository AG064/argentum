"use strict";
/**
 * Graph Memory Module
 *
 * OMEGA Memory integration — graph-based memory connections.
 * Links memories via typed edges for traversal and pathfinding.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.MemoryGraph = void 0;
exports.getMemoryGraph = getMemoryGraph;
const semantic_1 = require("./semantic");
/**
 * MemoryGraph — typed edge connections between memories.
 *
 * Supports BFS traversal, path finding, and connected-node queries
 * on top of the semantic memory store.
 */
class MemoryGraph {
    db;
    constructor(dbPath) {
        const semantic = (0, semantic_1.getSemanticMemory)(dbPath);
        this.db = semantic.getDb();
    }
    /** Add an edge between two memories */
    async addEdge(sourceId, targetId, relation, weight = 1.0) {
        // Verify both memories exist
        const source = this.db.prepare('SELECT id FROM memories WHERE id = ?').get(sourceId);
        const target = this.db.prepare('SELECT id FROM memories WHERE id = ?').get(targetId);
        if (!source || !target) {
            throw new Error(`Memory not found: ${!source ? sourceId : targetId}`);
        }
        // Upsert edge
        this.db.prepare(`
      INSERT INTO edges (source_id, target_id, relation_type, weight, created_at)
      VALUES (@source_id, @target_id, @relation_type, @weight, @created_at)
      ON CONFLICT(source_id, target_id, relation_type) DO UPDATE SET
        weight = @weight
    `).run({
            source_id: sourceId,
            target_id: targetId,
            relation_type: relation,
            weight,
            created_at: Date.now(),
        });
    }
    /** Remove an edge */
    async removeEdge(sourceId, targetId, relation) {
        let result;
        if (relation) {
            result = this.db.prepare('DELETE FROM edges WHERE source_id = ? AND target_id = ? AND relation_type = ?').run(sourceId, targetId, relation);
        }
        else {
            result = this.db.prepare('DELETE FROM edges WHERE source_id = ? AND target_id = ?').run(sourceId, targetId);
        }
        return result.changes > 0;
    }
    /** Traverse graph from a starting node (BFS) */
    async traverse(startId, options = {}) {
        const maxDepth = options.maxDepth ?? 3;
        const minWeight = options.minWeight ?? 0;
        const relationTypes = options.relationTypes;
        const visited = new Set();
        const result = [];
        const queue = [
            { id: startId, depth: 0 },
        ];
        while (queue.length > 0) {
            const current = queue.shift();
            if (visited.has(current.id))
                continue;
            visited.add(current.id);
            // Get memory node
            const mem = this.db.prepare('SELECT * FROM memories WHERE id = ?').get(current.id);
            if (mem) {
                result.push({
                    id: mem['id'],
                    type: mem['type'],
                    content: mem['content'],
                    weight: mem['weight'],
                    depth: current.depth,
                    relation: current.relation,
                    metadata: this.parseJson(mem['metadata']),
                });
            }
            // Stop at max depth
            if (current.depth >= maxDepth)
                continue;
            // Get outgoing edges
            let edgeQuery = 'SELECT * FROM edges WHERE source_id = ? AND weight >= ?';
            const params = [current.id, minWeight];
            if (relationTypes && relationTypes.length > 0) {
                const placeholders = relationTypes.map(() => '?').join(',');
                edgeQuery += ` AND relation_type IN (${placeholders})`;
                params.push(...relationTypes);
            }
            const edges = this.db.prepare(edgeQuery).all(...params);
            for (const edge of edges) {
                if (!visited.has(edge.target_id)) {
                    queue.push({
                        id: edge.target_id,
                        depth: current.depth + 1,
                        relation: edge.relation_type,
                    });
                }
            }
        }
        return result;
    }
    /** Find a path between two memories (BFS) */
    async findPath(sourceId, targetId, maxDepth = 5) {
        const visited = new Map();
        const queue = [sourceId];
        visited.set(sourceId, { parent: '', relation: '' });
        while (queue.length > 0) {
            const currentId = queue.shift();
            if (currentId === targetId) {
                // Reconstruct path
                return this.reconstructPath(visited, targetId);
            }
            const currentDepth = this.getPathDepth(visited, currentId);
            if (currentDepth >= maxDepth)
                continue;
            // Get all edges from current node (both directions)
            const edges = this.db.prepare(`SELECT target_id as neighbor_id, relation_type FROM edges WHERE source_id = ?
         UNION
         SELECT source_id as neighbor_id, relation_type FROM edges WHERE target_id = ?`).all(currentId, currentId);
            for (const edge of edges) {
                if (!visited.has(edge.neighbor_id)) {
                    visited.set(edge.neighbor_id, {
                        parent: currentId,
                        relation: edge.relation_type,
                    });
                    queue.push(edge.neighbor_id);
                }
            }
        }
        return []; // No path found
    }
    /** Get all memories directly connected to a given memory */
    async getConnected(id) {
        const rows = this.db.prepare(`
      SELECT DISTINCT m.* FROM memories m
      WHERE m.id IN (
        SELECT target_id FROM edges WHERE source_id = ?
        UNION
        SELECT source_id FROM edges WHERE target_id = ?
      )
      ORDER BY m.created_at DESC
    `).all(id, id);
        return rows.map(r => ({
            id: r['id'],
            type: r['type'],
            content: r['content'],
            embedding: r['embedding'],
            created_at: r['created_at'],
            accessed_at: r['accessed_at'],
            access_count: r['access_count'],
            metadata: this.parseJson(r['metadata']),
        }));
    }
    /** Get edges for a specific memory */
    async getEdges(id) {
        return this.db.prepare('SELECT * FROM edges WHERE source_id = ? OR target_id = ? ORDER BY weight DESC').all(id, id);
    }
    /** Get statistics about the graph */
    async getStats() {
        const nodes = this.db.prepare('SELECT COUNT(*) as c FROM memories').get().c;
        const edges = this.db.prepare('SELECT COUNT(*) as c FROM edges').get().c;
        const avgDegree = nodes > 0 ? (edges * 2) / nodes : 0;
        const relationRows = this.db.prepare('SELECT DISTINCT relation_type FROM edges').all();
        return {
            nodes,
            edges,
            avgDegree: Math.round(avgDegree * 100) / 100,
            relationTypes: relationRows.map(r => r.relation_type),
        };
    }
    /** Find memories that share many connections (community detection, simple) */
    async findCommunities(minSharedEdges = 2) {
        const allEdges = this.db.prepare('SELECT source_id, target_id FROM edges').all();
        // Build adjacency map
        const adjacency = new Map();
        for (const edge of allEdges) {
            if (!adjacency.has(edge.source_id))
                adjacency.set(edge.source_id, new Set());
            if (!adjacency.has(edge.target_id))
                adjacency.set(edge.target_id, new Set());
            adjacency.get(edge.source_id).add(edge.target_id);
            adjacency.get(edge.target_id).add(edge.source_id);
        }
        // Find pairs with shared neighbors
        const communities = [];
        const processed = new Set();
        for (const [nodeA, neighborsA] of adjacency) {
            for (const [nodeB, neighborsB] of adjacency) {
                if (nodeA >= nodeB || processed.has(`${nodeA}:${nodeB}`))
                    continue;
                const shared = [...neighborsA].filter(n => neighborsB.has(n)).length;
                if (shared >= minSharedEdges) {
                    communities.push({
                        memories: [nodeA, nodeB, ...[...neighborsA].filter(n => neighborsB.has(n))],
                        sharedEdges: shared,
                    });
                    processed.add(`${nodeA}:${nodeB}`);
                }
            }
        }
        return communities.sort((a, b) => b.sharedEdges - a.sharedEdges);
    }
    /** Reconstruct path from BFS visited map */
    reconstructPath(visited, targetId) {
        const path = [];
        let currentId = targetId;
        let depth = 0;
        while (currentId) {
            const mem = this.db.prepare('SELECT * FROM memories WHERE id = ?').get(currentId);
            if (mem) {
                const entry = visited.get(currentId);
                path.unshift({
                    id: mem['id'],
                    type: mem['type'],
                    content: mem['content'],
                    weight: mem['weight'],
                    depth,
                    relation: entry?.relation,
                    metadata: this.parseJson(mem['metadata']),
                });
            }
            const entry = visited.get(currentId);
            currentId = entry?.parent || undefined;
            depth++;
            // Safety: don't loop forever
            if (depth > 100)
                break;
        }
        return path;
    }
    /** Get depth of a path from BFS map */
    getPathDepth(visited, nodeId) {
        let depth = 0;
        let current = nodeId;
        const seen = new Set();
        while (visited.has(current) && !seen.has(current)) {
            seen.add(current);
            const entry = visited.get(current);
            if (!entry.parent)
                break;
            current = entry.parent;
            depth++;
        }
        return depth;
    }
    /** Safe JSON parse */
    parseJson(str) {
        try {
            return JSON.parse(str);
        }
        catch {
            return {};
        }
    }
}
exports.MemoryGraph = MemoryGraph;
// Singleton
let instance = null;
function getMemoryGraph(dbPath) {
    if (!instance) {
        instance = new MemoryGraph(dbPath);
    }
    return instance;
}
//# sourceMappingURL=graph.js.map