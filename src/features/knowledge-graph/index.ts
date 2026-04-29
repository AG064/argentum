/**
 * Knowledge Graph Feature
 *
 * Entity-relationship knowledge graph with file import (Markdown, JSON),
 * graph export, BFS pathfinding, and SQLite + in-memory backends.
 */

import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname } from 'path';

import Database from 'better-sqlite3';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

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
  stats: { entities: number; relationships: number };
}

/** Graph storage backend interface */
interface GraphBackend {
  init(): Promise<void>;
  close(): Promise<void>;
  addEntity(entity: Omit<Entity, 'id' | 'createdAt' | 'updatedAt'>): Promise<Entity>;
  getEntity(id: string): Promise<Entity | null>;
  updateEntity(id: string, updates: Partial<Entity>): Promise<Entity>;
  deleteEntity(id: string): Promise<void>;
  addRelationship(rel: Omit<Relationship, 'id' | 'createdAt'>): Promise<Relationship>;
  getRelationships(entityId: string, type?: string): Promise<Relationship[]>;
  getAllRelationships(): Promise<Relationship[]>;
  findEntities(query: { type?: string; name?: string; tags?: string[] }): Promise<Entity[]>;
  getAllEntities(): Promise<Entity[]>;
  findPaths(sourceId: string, targetId: string, maxDepth?: number): Promise<Entity[][]>;
  getStats(): Promise<{ entities: number; relationships: number }>;
  clear(): Promise<void>;
}

// ─── Markdown Import Parser ──────────────────────────────────────────────────

interface MarkdownEntity {
  name: string;
  type: string;
  properties: Record<string, string>;
  relations: Array<{ type: string; target: string; weight?: number }>;
}

function parseMarkdown(content: string): MarkdownEntity[] {
  const entities: MarkdownEntity[] = [];
  const lines = content.split('\n');
  let current: MarkdownEntity | null = null;

  for (const line of lines) {
    // ## Entity Name (type: Person)
    const headerMatch = line.match(/^##\s+(.+?)(?:\s*\((.+?)\))?$/);
    if (headerMatch) {
      if (current) entities.push(current);
      const typeMatch = headerMatch[2]?.match(/type:\s*(.+)/i);
      current = {
        name: headerMatch[1]!.trim(),
        type: typeMatch?.[1]?.trim() ?? 'unknown',
        properties: {},
        relations: [],
      };
      continue;
    }

    if (!current) continue;

    // - **key**: value  (property)
    const propMatch = line.match(/^\s*-\s*\*\*(.+?)\*\*:\s*(.+)$/);
    if (propMatch) {
      current.properties[propMatch[1]!.trim()] = propMatch[2]!.trim();
      continue;
    }

    // - relates_to: Other Entity (weight: 0.8)
    const relMatch = line.match(/^\s*-\s*(.+?):\s*(.+?)(?:\s*\((.+?)\))?$/);
    if (relMatch && !relMatch[1]!.startsWith('**')) {
      const weightMatch = relMatch[3]?.match(/weight:\s*([\d.]+)/);
      current.relations.push({
        type: relMatch[1]!.trim().replace(/\s+/g, '_'),
        target: relMatch[2]!.trim(),
        weight: weightMatch ? parseFloat(weightMatch[1]!) : 1.0,
      });
    }
  }
  if (current) entities.push(current);
  return entities;
}

// ─── SQLite Backend ──────────────────────────────────────────────────────────

class SQLiteGraphBackend implements GraphBackend {
  private db!: Database.Database;

  constructor(private path: string) {}

  async init(): Promise<void> {
    mkdirSync(dirname(this.path), { recursive: true });
    this.db = new Database(this.path);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('foreign_keys = ON');
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS entities (
        id TEXT PRIMARY KEY, type TEXT NOT NULL, name TEXT NOT NULL,
        properties TEXT DEFAULT '{}', tags TEXT DEFAULT '[]',
        created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(type);
      CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name);

      CREATE TABLE IF NOT EXISTS relationships (
        id TEXT PRIMARY KEY, source_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
        target_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
        type TEXT NOT NULL, properties TEXT DEFAULT '{}', weight REAL DEFAULT 1.0,
        created_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_rel_source ON relationships(source_id);
      CREATE INDEX IF NOT EXISTS idx_rel_target ON relationships(target_id);
      CREATE INDEX IF NOT EXISTS idx_rel_type ON relationships(type);
    `);
  }

  async close(): Promise<void> {
    this.db?.close();
  }

  async addEntity(data: Omit<Entity, 'id' | 'createdAt' | 'updatedAt'>): Promise<Entity> {
    const id = `ent_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const now = Date.now();
    this.db
      .prepare(
        'INSERT INTO entities (id, type, name, properties, tags, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)',
      )
      .run(
        id,
        data.type,
        data.name,
        JSON.stringify(data.properties),
        JSON.stringify(data.tags ?? []),
        now,
        now,
      );
    return { id, ...data, tags: data.tags ?? [], createdAt: now, updatedAt: now };
  }

  async getEntity(id: string): Promise<Entity | null> {
    const row = this.db.prepare('SELECT * FROM entities WHERE id = ?').get(id) as
      | Record<string, unknown>
      | undefined;
    if (!row) return null;
    return {
      id: row['id'] as string,
      type: row['type'] as string,
      name: row['name'] as string,
      properties: JSON.parse(row['properties'] as string),
      tags: JSON.parse((row['tags'] as string) ?? '[]'),
      createdAt: row['created_at'] as number,
      updatedAt: row['updated_at'] as number,
    };
  }

  async updateEntity(id: string, updates: Partial<Entity>): Promise<Entity> {
    const existing = await this.getEntity(id);
    if (!existing) throw new Error(`Entity not found: ${id}`);
    const now = Date.now();
    const merged = { ...existing, ...updates, updatedAt: now };
    this.db
      .prepare('UPDATE entities SET name=?, properties=?, tags=?, updated_at=? WHERE id=?')
      .run(merged.name, JSON.stringify(merged.properties), JSON.stringify(merged.tags), now, id);
    return merged;
  }

  async deleteEntity(id: string): Promise<void> {
    this.db.prepare('DELETE FROM relationships WHERE source_id = ? OR target_id = ?').run(id, id);
    this.db.prepare('DELETE FROM entities WHERE id = ?').run(id);
  }

  async addRelationship(data: Omit<Relationship, 'id' | 'createdAt'>): Promise<Relationship> {
    const id = `rel_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const now = Date.now();
    this.db
      .prepare(
        'INSERT INTO relationships (id, source_id, target_id, type, properties, weight, created_at) VALUES (?,?,?,?,?,?,?)',
      )
      .run(
        id,
        data.sourceId,
        data.targetId,
        data.type,
        JSON.stringify(data.properties),
        data.weight,
        now,
      );
    return { id, ...data, createdAt: now };
  }

  async getRelationships(entityId: string, type?: string): Promise<Relationship[]> {
    let query = 'SELECT * FROM relationships WHERE source_id = ? OR target_id = ?';
    const params: unknown[] = [entityId, entityId];
    if (type) {
      query += ' AND type = ?';
      params.push(type);
    }
    const rows = this.db.prepare(query).all(...params) as Record<string, unknown>[];
    return rows.map(this.rowToRel);
  }

  async getAllRelationships(): Promise<Relationship[]> {
    return (this.db.prepare('SELECT * FROM relationships').all() as Record<string, unknown>[]).map(
      this.rowToRel,
    );
  }

  async findEntities(query: { type?: string; name?: string; tags?: string[] }): Promise<Entity[]> {
    let sql = 'SELECT * FROM entities WHERE 1=1';
    const params: unknown[] = [];
    if (query.type) {
      sql += ' AND type = ?';
      params.push(query.type);
    }
    if (query.name) {
      sql += ' AND name LIKE ?';
      params.push(`%${query.name}%`);
    }
    if (query.tags?.length) {
      // Simple JSON array search
      for (const tag of query.tags) {
        sql += ' AND tags LIKE ?';
        params.push(`%"${tag}"%`);
      }
    }
    return (this.db.prepare(sql).all(...params) as Record<string, unknown>[]).map(this.rowToEntity);
  }

  async getAllEntities(): Promise<Entity[]> {
    return (this.db.prepare('SELECT * FROM entities').all() as Record<string, unknown>[]).map(
      this.rowToEntity,
    );
  }

  async findPaths(sourceId: string, targetId: string, maxDepth = 5): Promise<Entity[][]> {
    const visited = new Set<string>();
    const queue: Array<{ id: string; path: string[] }> = [{ id: sourceId, path: [sourceId] }];
    const paths: Entity[][] = [];

    while (queue.length > 0 && paths.length < 10) {
      const { id, path } = queue.shift()!;
      if (id === targetId) {
        const entities = await Promise.all(path.map((pid) => this.getEntity(pid)));
        paths.push(entities.filter((e): e is Entity => e !== null));
        continue;
      }
      if (path.length >= maxDepth || visited.has(id)) continue;
      visited.add(id);

      const rels = await this.getRelationships(id);
      for (const rel of rels) {
        const nextId = rel.sourceId === id ? rel.targetId : rel.sourceId;
        if (!visited.has(nextId)) queue.push({ id: nextId, path: [...path, nextId] });
      }
    }
    return paths;
  }

  async getStats(): Promise<{ entities: number; relationships: number }> {
    const entities = (this.db.prepare('SELECT COUNT(*) as c FROM entities').get() as { c: number })
      .c;
    const relationships = (
      this.db.prepare('SELECT COUNT(*) as c FROM relationships').get() as { c: number }
    ).c;
    return { entities, relationships };
  }

  async clear(): Promise<void> {
    this.db.exec('DELETE FROM relationships; DELETE FROM entities;');
  }

  private rowToEntity(row: Record<string, unknown>): Entity {
    return {
      id: row['id'] as string,
      type: row['type'] as string,
      name: row['name'] as string,
      properties: JSON.parse(row['properties'] as string),
      tags: JSON.parse((row['tags'] as string) ?? '[]'),
      createdAt: row['created_at'] as number,
      updatedAt: row['updated_at'] as number,
    };
  }

  private rowToRel(row: Record<string, unknown>): Relationship {
    return {
      id: row['id'] as string,
      sourceId: row['source_id'] as string,
      targetId: row['target_id'] as string,
      type: row['type'] as string,
      properties: JSON.parse(row['properties'] as string),
      weight: row['weight'] as number,
      createdAt: row['created_at'] as number,
    };
  }
}

// ─── In-Memory Backend ───────────────────────────────────────────────────────

class MemoryGraphBackend implements GraphBackend {
  private entities: Map<string, Entity> = new Map();
  private relationships: Map<string, Relationship> = new Map();

  async init(): Promise<void> {}
  async close(): Promise<void> {}

  async addEntity(data: Omit<Entity, 'id' | 'createdAt' | 'updatedAt'>): Promise<Entity> {
    const id = `ent_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const now = Date.now();
    const entity: Entity = { id, ...data, tags: data.tags ?? [], createdAt: now, updatedAt: now };
    this.entities.set(id, entity);
    return entity;
  }

  async getEntity(id: string): Promise<Entity | null> {
    return this.entities.get(id) ?? null;
  }

  async updateEntity(id: string, updates: Partial<Entity>): Promise<Entity> {
    const existing = this.entities.get(id);
    if (!existing) throw new Error(`Entity not found: ${id}`);
    const updated = { ...existing, ...updates, updatedAt: Date.now() };
    this.entities.set(id, updated);
    return updated;
  }

  async deleteEntity(id: string): Promise<void> {
    this.entities.delete(id);
    for (const [rid, rel] of this.relationships) {
      if (rel.sourceId === id || rel.targetId === id) this.relationships.delete(rid);
    }
  }

  async addRelationship(data: Omit<Relationship, 'id' | 'createdAt'>): Promise<Relationship> {
    const id = `rel_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const rel: Relationship = { id, ...data, createdAt: Date.now() };
    this.relationships.set(id, rel);
    return rel;
  }

  async getRelationships(entityId: string, type?: string): Promise<Relationship[]> {
    return Array.from(this.relationships.values()).filter(
      (r) => (r.sourceId === entityId || r.targetId === entityId) && (!type || r.type === type),
    );
  }

  async getAllRelationships(): Promise<Relationship[]> {
    return Array.from(this.relationships.values());
  }
  async getAllEntities(): Promise<Entity[]> {
    return Array.from(this.entities.values());
  }

  async findEntities(query: { type?: string; name?: string; tags?: string[] }): Promise<Entity[]> {
    return Array.from(this.entities.values()).filter((e) => {
      if (query.type && e.type !== query.type) return false;
      if (query.name && !e.name.toLowerCase().includes(query.name.toLowerCase())) return false;
      if (query.tags?.length && !query.tags.some((t) => e.tags.includes(t))) return false;
      return true;
    });
  }

  async findPaths(sourceId: string, targetId: string, maxDepth = 5): Promise<Entity[][]> {
    const visited = new Set<string>();
    const queue: Array<{ id: string; path: string[] }> = [{ id: sourceId, path: [sourceId] }];
    const paths: Entity[][] = [];
    while (queue.length > 0 && paths.length < 10) {
      const { id, path } = queue.shift()!;
      if (id === targetId) {
        paths.push(path.map((pid) => this.entities.get(pid)!).filter(Boolean));
        continue;
      }
      if (path.length >= maxDepth || visited.has(id)) continue;
      visited.add(id);
      for (const rel of await this.getRelationships(id)) {
        const nextId = rel.sourceId === id ? rel.targetId : rel.sourceId;
        if (!visited.has(nextId)) queue.push({ id: nextId, path: [...path, nextId] });
      }
    }
    return paths;
  }

  async getStats(): Promise<{ entities: number; relationships: number }> {
    return { entities: this.entities.size, relationships: this.relationships.size };
  }

  async clear(): Promise<void> {
    this.entities.clear();
    this.relationships.clear();
  }
}

// ─── Main Feature ────────────────────────────────────────────────────────────

/**
 * Knowledge Graph feature — entity-relationship graph with file import/export.
 */
class KnowledgeGraphFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'knowledge-graph',
    version: '0.0.3',
    description: 'Knowledge graph with Markdown/JSON import, export, pathfinding',
    dependencies: [],
  };

  private config: KnowledgeGraphConfig = {
    enabled: false,
    backend: 'sqlite',
    path: './data/knowledge.db',
    importDir: './data/knowledge/imports',
    exportDir: './data/knowledge/exports',
  };
  private ctx!: FeatureContext;
  private backend: GraphBackend | null = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<KnowledgeGraphConfig>) };
  }

  async start(): Promise<void> {
    this.backend =
      this.config.backend === 'sqlite'
        ? new SQLiteGraphBackend(this.config.path)
        : new MemoryGraphBackend();
    await this.backend.init();
    this.ctx.logger.info('Knowledge Graph active', { backend: this.config.backend });
  }

  async stop(): Promise<void> {
    await this.backend?.close();
    this.backend = null;
  }

  async healthCheck(): Promise<HealthStatus> {
    if (!this.backend) return { healthy: false, message: 'Not initialized' };
    const stats = await this.backend.getStats();
    return { healthy: true, details: stats };
  }

  // ─── CRUD ─────────────────────────────────────────────────────────────────

  async addEntity(
    type: string,
    name: string,
    properties: Record<string, unknown> = {},
    tags: string[] = [],
  ): Promise<Entity> {
    if (!this.backend) throw new Error('Backend not initialized');
    return this.backend.addEntity({ type, name, properties, tags });
  }

  async getEntity(id: string): Promise<Entity | null> {
    if (!this.backend) throw new Error('Backend not initialized');
    return this.backend.getEntity(id);
  }

  async updateEntity(id: string, updates: Partial<Entity>): Promise<Entity> {
    if (!this.backend) throw new Error('Backend not initialized');
    return this.backend.updateEntity(id, updates);
  }

  async deleteEntity(id: string): Promise<void> {
    if (!this.backend) throw new Error('Backend not initialized');
    return this.backend.deleteEntity(id);
  }

  async addRelationship(
    sourceId: string,
    targetId: string,
    type: string,
    properties: Record<string, unknown> = {},
    weight = 1.0,
  ): Promise<Relationship> {
    if (!this.backend) throw new Error('Backend not initialized');
    return this.backend.addRelationship({ sourceId, targetId, type, properties, weight });
  }

  async findEntities(query: { type?: string; name?: string; tags?: string[] }): Promise<Entity[]> {
    if (!this.backend) throw new Error('Backend not initialized');
    return this.backend.findEntities(query);
  }

  async findPaths(sourceId: string, targetId: string, maxDepth = 5): Promise<Entity[][]> {
    if (!this.backend) throw new Error('Backend not initialized');
    return this.backend.findPaths(sourceId, targetId, maxDepth);
  }

  // ─── 3D Visualization Data ───────────────────────────────────────────────

  /**
   * Get graph data formatted for 3D visualization
   * Returns {nodes, links} format compatible with 3d-force-graph
   */
  async getGraph3DData(): Promise<{
    entities: Entity[];
    relationships: Relationship[];
  }> {
    if (!this.backend) throw new Error('Backend not initialized');
    const entities = await this.backend.getAllEntities();
    const relationships = await this.backend.getAllRelationships();
    return { entities, relationships };
  }

  // ─── Import ───────────────────────────────────────────────────────────────

  /** Import from Markdown file */
  async importFromMarkdown(filePath: string): Promise<{ entities: number; relationships: number }> {
    if (!this.backend) throw new Error('Backend not initialized');
    const content = readFileSync(filePath, 'utf-8');
    const parsed = parseMarkdown(content);
    let entityCount = 0;
    let relCount = 0;
    const nameToId = new Map<string, string>();

    for (const item of parsed) {
      const entity = await this.backend.addEntity({
        type: item.type,
        name: item.name,
        properties: item.properties,
        tags: [],
      });
      nameToId.set(item.name, entity.id);
      entityCount++;
    }

    for (const item of parsed) {
      const sourceId = nameToId.get(item.name);
      if (!sourceId) continue;
      for (const rel of item.relations) {
        const targetId = nameToId.get(rel.target);
        if (!targetId) continue;
        await this.backend.addRelationship({
          sourceId,
          targetId,
          type: rel.type,
          properties: {},
          weight: rel.weight ?? 1.0,
        });
        relCount++;
      }
    }

    this.ctx.logger.info('Markdown import complete', {
      file: filePath,
      entities: entityCount,
      relationships: relCount,
    });
    return { entities: entityCount, relationships: relCount };
  }

  /** Import from JSON file */
  async importFromJson(filePath: string): Promise<{ entities: number; relationships: number }> {
    if (!this.backend) throw new Error('Backend not initialized');
    const data = JSON.parse(readFileSync(filePath, 'utf-8')) as {
      entities?: Array<Omit<Entity, 'id' | 'createdAt' | 'updatedAt'>>;
      relationships?: Array<Omit<Relationship, 'id' | 'createdAt'>>;
    };
    let entityCount = 0;
    let relCount = 0;

    for (const ent of data.entities ?? []) {
      await this.backend.addEntity(ent);
      entityCount++;
    }

    for (const rel of data.relationships ?? []) {
      await this.backend.addRelationship(rel);
      relCount++;
    }

    this.ctx.logger.info('JSON import complete', {
      file: filePath,
      entities: entityCount,
      relationships: relCount,
    });
    return { entities: entityCount, relationships: relCount };
  }

  /** Import from any supported file */
  async importFromFile(filePath: string): Promise<{ entities: number; relationships: number }> {
    if (filePath.endsWith('.md') || filePath.endsWith('.markdown'))
      return this.importFromMarkdown(filePath);
    if (filePath.endsWith('.json')) return this.importFromJson(filePath);
    throw new Error(`Unsupported file format: ${filePath}`);
  }

  // ─── Export ───────────────────────────────────────────────────────────────

  /** Export as JSON graph data */
  async exportGraph(): Promise<GraphData> {
    if (!this.backend) throw new Error('Backend not initialized');
    const entities = await this.backend.getAllEntities();
    const relationships = await this.backend.getAllRelationships();
    const stats = await this.backend.getStats();
    return { entities, relationships, exportedAt: Date.now(), stats };
  }

  /** Export to JSON file */
  async exportToFile(filePath: string): Promise<GraphData> {
    const data = await this.exportGraph();
    mkdirSync(dirname(filePath), { recursive: true });
    writeFileSync(filePath, JSON.stringify(data, null, 2), 'utf-8');
    this.ctx.logger.info('Graph exported', { file: filePath, ...data.stats });
    return data;
  }
}

export default new KnowledgeGraphFeature();
