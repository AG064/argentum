/**
 * Graph Memory Module
 *
 * OMEGA Memory integration — graph-based memory connections.
 * Links memories via typed edges for traversal and pathfinding.
 */

import Database from 'better-sqlite3';
import { getSemanticMemory, MemoryResult } from './semantic';

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
export class MemoryGraph {
  private db!: Database.Database;

  constructor(dbPath?: string) {
    const semantic = getSemanticMemory(dbPath);
    this.db = semantic.getDb();
  }

  /** Add an edge between two memories */
  async addEdge(
    sourceId: string,
    targetId: string,
    relation: string,
    weight = 1.0
  ): Promise<void> {
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
  async removeEdge(sourceId: string, targetId: string, relation?: string): Promise<boolean> {
    let result;
    if (relation) {
      result = this.db.prepare(
        'DELETE FROM edges WHERE source_id = ? AND target_id = ? AND relation_type = ?'
      ).run(sourceId, targetId, relation);
    } else {
      result = this.db.prepare(
        'DELETE FROM edges WHERE source_id = ? AND target_id = ?'
      ).run(sourceId, targetId);
    }
    return result.changes > 0;
  }

  /** Traverse graph from a starting node (BFS) */
  async traverse(startId: string, options: TraverseOptions = {}): Promise<MemoryNode[]> {
    const maxDepth = options.maxDepth ?? 3;
    const minWeight = options.minWeight ?? 0;
    const relationTypes = options.relationTypes;

    const visited = new Set<string>();
    const result: MemoryNode[] = [];
    const queue: Array<{ id: string; depth: number; relation?: string }> = [
      { id: startId, depth: 0 },
    ];

    while (queue.length > 0) {
      const current = queue.shift()!;

      if (visited.has(current.id)) continue;
      visited.add(current.id);

      // Get memory node
      const mem = this.db.prepare(
        'SELECT * FROM memories WHERE id = ?'
      ).get(current.id) as Record<string, unknown> | undefined;

      if (mem) {
        result.push({
          id: mem['id'] as string,
          type: mem['type'] as string,
          content: mem['content'] as string,
          weight: mem['weight'] as number,
          depth: current.depth,
          relation: current.relation,
          metadata: this.parseJson(mem['metadata'] as string),
        });
      }

      // Stop at max depth
      if (current.depth >= maxDepth) continue;

      // Get outgoing edges
      let edgeQuery = 'SELECT * FROM edges WHERE source_id = ? AND weight >= ?';
      const params: unknown[] = [current.id, minWeight];

      if (relationTypes && relationTypes.length > 0) {
        const placeholders = relationTypes.map(() => '?').join(',');
        edgeQuery += ` AND relation_type IN (${placeholders})`;
        params.push(...relationTypes);
      }

      const edges = this.db.prepare(edgeQuery).all(...params) as Array<{
        target_id: string;
        relation_type: string;
      }>;

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
  async findPath(sourceId: string, targetId: string, maxDepth = 5): Promise<MemoryNode[]> {
    const visited = new Map<string, { parent: string; relation: string }>();
    const queue: string[] = [sourceId];
    visited.set(sourceId, { parent: '', relation: '' });

    while (queue.length > 0) {
      const currentId = queue.shift()!;

      if (currentId === targetId) {
        // Reconstruct path
        return this.reconstructPath(visited, targetId);
      }

      const currentDepth = this.getPathDepth(visited, currentId);

      if (currentDepth >= maxDepth) continue;

      // Get all edges from current node (both directions)
      const edges = this.db.prepare(
        `SELECT target_id as neighbor_id, relation_type FROM edges WHERE source_id = ?
         UNION
         SELECT source_id as neighbor_id, relation_type FROM edges WHERE target_id = ?`
      ).all(currentId, currentId) as Array<{ neighbor_id: string; relation_type: string }>;

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
  async getConnected(id: string): Promise<MemoryResult[]> {
    const rows = this.db.prepare(`
      SELECT DISTINCT m.* FROM memories m
      WHERE m.id IN (
        SELECT target_id FROM edges WHERE source_id = ?
        UNION
        SELECT source_id FROM edges WHERE target_id = ?
      )
      ORDER BY m.created_at DESC
    `).all(id, id) as Array<Record<string, unknown>>;

    return rows.map(r => ({
      id: r['id'] as string,
      type: r['type'] as string,
      content: r['content'] as string,
      embedding: r['embedding'] as Buffer | null,
      created_at: r['created_at'] as number,
      accessed_at: r['accessed_at'] as number,
      access_count: r['access_count'] as number,
      metadata: this.parseJson(r['metadata'] as string),
    }));
  }

  /** Get edges for a specific memory */
  async getEdges(id: string): Promise<Array<{
    source_id: string;
    target_id: string;
    relation_type: string;
    weight: number;
  }>> {
    return this.db.prepare(
      'SELECT * FROM edges WHERE source_id = ? OR target_id = ? ORDER BY weight DESC'
    ).all(id, id) as Array<{
      source_id: string;
      target_id: string;
      relation_type: string;
      weight: number;
    }>;
  }

  /** Get statistics about the graph */
  async getStats(): Promise<{
    nodes: number;
    edges: number;
    avgDegree: number;
    relationTypes: string[];
  }> {
    const nodes = (this.db.prepare('SELECT COUNT(*) as c FROM memories').get() as { c: number }).c;
    const edges = (this.db.prepare('SELECT COUNT(*) as c FROM edges').get() as { c: number }).c;
    const avgDegree = nodes > 0 ? (edges * 2) / nodes : 0;

    const relationRows = this.db.prepare(
      'SELECT DISTINCT relation_type FROM edges'
    ).all() as Array<{ relation_type: string }>;

    return {
      nodes,
      edges,
      avgDegree: Math.round(avgDegree * 100) / 100,
      relationTypes: relationRows.map(r => r.relation_type),
    };
  }

  /** Find memories that share many connections (community detection, simple) */
  async findCommunities(minSharedEdges = 2): Promise<Array<{
    memories: string[];
    sharedEdges: number;
  }>> {
    const allEdges = this.db.prepare('SELECT source_id, target_id FROM edges').all() as Array<{
      source_id: string;
      target_id: string;
    }>;

    // Build adjacency map
    const adjacency = new Map<string, Set<string>>();
    for (const edge of allEdges) {
      if (!adjacency.has(edge.source_id)) adjacency.set(edge.source_id, new Set());
      if (!adjacency.has(edge.target_id)) adjacency.set(edge.target_id, new Set());
      adjacency.get(edge.source_id)!.add(edge.target_id);
      adjacency.get(edge.target_id)!.add(edge.source_id);
    }

    // Find pairs with shared neighbors
    const communities: Array<{ memories: string[]; sharedEdges: number }> = [];
    const processed = new Set<string>();

    for (const [nodeA, neighborsA] of adjacency) {
      for (const [nodeB, neighborsB] of adjacency) {
        if (nodeA >= nodeB || processed.has(`${nodeA}:${nodeB}`)) continue;

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
  private reconstructPath(
    visited: Map<string, { parent: string; relation: string }>,
    targetId: string
  ): MemoryNode[] {
    const path: MemoryNode[] = [];
    let currentId: string | undefined = targetId;
    let depth = 0;

    while (currentId) {
      const mem = this.db.prepare('SELECT * FROM memories WHERE id = ?').get(currentId) as
        Record<string, unknown> | undefined;

      if (mem) {
        const entry = visited.get(currentId);
        path.unshift({
          id: mem['id'] as string,
          type: mem['type'] as string,
          content: mem['content'] as string,
          weight: mem['weight'] as number,
          depth,
          relation: entry?.relation,
          metadata: this.parseJson(mem['metadata'] as string),
        });
      }

      const entry = visited.get(currentId);
      currentId = entry?.parent || undefined;
      depth++;

      // Safety: don't loop forever
      if (depth > 100) break;
    }

    return path;
  }

  /** Get depth of a path from BFS map */
  private getPathDepth(
    visited: Map<string, { parent: string; relation: string }>,
    nodeId: string
  ): number {
    let depth = 0;
    let current = nodeId;
    const seen = new Set<string>();

    while (visited.has(current) && !seen.has(current)) {
      seen.add(current);
      const entry = visited.get(current)!;
      if (!entry.parent) break;
      current = entry.parent;
      depth++;
    }

    return depth;
  }

  /** Safe JSON parse */
  private parseJson(str: string): Record<string, unknown> {
    try {
      return JSON.parse(str);
    } catch {
      return {};
    }
  }
}

// Singleton
let instance: MemoryGraph | null = null;

export function getMemoryGraph(dbPath?: string): MemoryGraph {
  if (!instance) {
    instance = new MemoryGraph(dbPath);
  }
  return instance;
}
