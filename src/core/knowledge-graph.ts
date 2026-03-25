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

import Database from 'better-sqlite3';
import { existsSync, mkdirSync } from 'fs';
import { dirname, resolve } from 'path';

import { featureLogger } from './logger';
import { generateId } from '../utils/id';

const log = featureLogger('knowledge-graph');

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface Entity {
  id: string;
  type: string; // 'person', 'project', 'concept', etc.
  name: string;
  properties: Record<string, unknown>;
  observations: string[];
  createdAt: number;
  updatedAt: number;
}

export interface Relation {
  id: string;
  source: string; // entity id
  target: string; // entity id
  type: string; // 'owns', 'depends_on', 'implements', etc.
  properties: Record<string, unknown>;
}

export interface BitemporalVersion {
  entityId: string;
  validTime: { start: number; end?: number };
  transactionTime: { start: number; end?: number };
  snapshot: Entity;
}

type Scope = 'global' | 'project' | 'task';

// ---------------------------------------------------------------------------
// KnowledgeGraphMemory
// ---------------------------------------------------------------------------

export class KnowledgeGraphMemory {
  private db: Database.Database;
  private scope: Scope = 'global';
  private scopeId: string | null = null;

  constructor(dbPath?: string) {
    const resolved = resolve(dbPath ?? './data/knowledge-graph.db');
    const dir = dirname(resolved);
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }

    this.db = new Database(resolved);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('foreign_keys = ON');
    this.initSchema();
  }

  // -------------------------------------------------------------------------
  // Schema
  // -------------------------------------------------------------------------

  private initSchema(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS entities (
        id          TEXT PRIMARY KEY,
        type        TEXT NOT NULL,
        name        TEXT NOT NULL,
        properties  TEXT NOT NULL DEFAULT '{}',
        observations TEXT NOT NULL DEFAULT '[]',
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL,
        scope       TEXT NOT NULL DEFAULT 'global',
        scope_id    TEXT
      );

      CREATE TABLE IF NOT EXISTS relations (
        id          TEXT PRIMARY KEY,
        source      TEXT NOT NULL,
        target      TEXT NOT NULL,
        type        TEXT NOT NULL,
        properties  TEXT NOT NULL DEFAULT '{}',
        created_at  INTEGER NOT NULL,
        scope       TEXT NOT NULL DEFAULT 'global',
        scope_id    TEXT,
        FOREIGN KEY (source) REFERENCES entities(id) ON DELETE CASCADE,
        FOREIGN KEY (target) REFERENCES entities(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS entity_versions (
        id              TEXT PRIMARY KEY,
        entity_id       TEXT NOT NULL,
        valid_start     INTEGER NOT NULL,
        valid_end       INTEGER,
        transaction_start INTEGER NOT NULL,
        transaction_end INTEGER,
        snapshot        TEXT NOT NULL,
        FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(type);
      CREATE INDEX IF NOT EXISTS idx_entities_scope ON entities(scope, scope_id);
      CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source);
      CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target);
      CREATE INDEX IF NOT EXISTS idx_relations_scope ON relations(scope, scope_id);
      CREATE INDEX IF NOT EXISTS idx_versions_entity ON entity_versions(entity_id);
      CREATE INDEX IF NOT EXISTS idx_versions_valid ON entity_versions(valid_start, valid_end);
      CREATE INDEX IF NOT EXISTS idx_versions_tx ON entity_versions(transaction_start, transaction_end);
    `);
  }

  // -------------------------------------------------------------------------
  // Scope
  // -------------------------------------------------------------------------

  setScope(scope: Scope, scopeId?: string): void {
    this.scope = scope;
    this.scopeId = scopeId ?? null;
    log.debug('Scope set', { scope, scopeId: this.scopeId });
  }

  getScope(): string {
    return this.scope;
  }

  // -------------------------------------------------------------------------
  // addEntity
  // -------------------------------------------------------------------------

  addEntity(
    entity: Omit<Entity, 'id' | 'createdAt' | 'updatedAt'>,
  ): string {
    const now = Date.now();
    const id = generateId();

    const insert = this.db.prepare(`
      INSERT INTO entities (id, type, name, properties, observations, created_at, updated_at, scope, scope_id)
      VALUES (@id, @type, @name, @properties, @observations, @createdAt, @updatedAt, @scope, @scopeId)
    `);

    insert.run({
      id,
      type: entity.type,
      name: entity.name,
      properties: JSON.stringify(entity.properties ?? {}),
      observations: JSON.stringify(entity.observations ?? []),
      createdAt: now,
      updatedAt: now,
      scope: this.scope,
      scopeId: this.scopeId,
    });

    // Record initial version
    this.recordVersion(id, entity as Entity, now, now);

    log.debug('Entity added', { id, type: entity.type, name: entity.name });
    return id;
  }

  // -------------------------------------------------------------------------
  // addRelation
  // -------------------------------------------------------------------------

  addRelation(relation: Omit<Relation, 'id'>): string {
    const now = Date.now();
    const id = generateId();

    const insert = this.db.prepare(`
      INSERT INTO relations (id, source, target, type, properties, created_at, scope, scope_id)
      VALUES (@id, @source, @target, @type, @properties, @createdAt, @scope, @scopeId)
    `);

    insert.run({
      id,
      source: relation.source,
      target: relation.target,
      type: relation.type,
      properties: JSON.stringify(relation.properties ?? {}),
      createdAt: now,
      scope: this.scope,
      scopeId: this.scopeId,
    });

    log.debug('Relation added', { id, type: relation.type, source: relation.source, target: relation.target });
    return id;
  }

  // -------------------------------------------------------------------------
  // addObservation — append to an entity's observations array
  // -------------------------------------------------------------------------

  addObservation(entityId: string, observation: string): void {
    const now = Date.now();

    const row = this.db.prepare(`SELECT observations FROM entities WHERE id = ?`).get(entityId) as
      | { observations: string }
      | undefined;

    if (!row) {
      log.warn('addObservation: entity not found', { entityId });
      return;
    }

    const observations: string[] = JSON.parse(row.observations);
    observations.push(observation);

    this.db.prepare(`UPDATE entities SET observations = ?, updated_at = ? WHERE id = ?`).run(
      JSON.stringify(observations),
      now,
      entityId,
    );

    // Record a new version for this observation
    const entity = this.getEntity(entityId);
    if (entity) {
      this.recordVersion(entityId, entity, now, now);
    }

    log.debug('Observation added', { entityId, observation });
  }

  // -------------------------------------------------------------------------
  // query — retrieve entities and optionally relations by type
  // -------------------------------------------------------------------------

  query(entities?: string[], relations?: string[]): Entity[] {
    let stmt;

    if (entities && entities.length > 0) {
      const placeholders = entities.map(() => '?').join(', ');
      const rows = this.db.prepare(`
        SELECT * FROM entities
        WHERE type IN (${placeholders})
          AND (scope = 'global' OR scope = ?)
          AND (scope != 'global' OR scope_id IS NULL OR scope_id = ?)
        ORDER BY created_at DESC
      `).all(...entities, this.scope, this.scopeId ?? '') as DbEntityRow[];

      return rows.map(this.rowToEntity);
    }

    const rows = this.db.prepare(`
      SELECT * FROM entities
      WHERE scope = 'global' OR scope = ?
      ORDER BY created_at DESC
    `).all(this.scope) as DbEntityRow[];

    return rows.map(this.rowToEntity);
  }

  // -------------------------------------------------------------------------
  // getEntity — fetch a single entity by id
  // -------------------------------------------------------------------------

  getEntity(id: string): Entity | null {
    const row = this.db.prepare(`SELECT * FROM entities WHERE id = ?`).get(id) as DbEntityRow | undefined;
    return row ? this.rowToEntity(row) : null;
  }

  // -------------------------------------------------------------------------
  // findPath — BFS graph traversal between two entity ids
  // -------------------------------------------------------------------------

  findPath(from: string, to: string, maxHops = 4): string[] {
    if (from === to) return [from];

    const visited = new Set<string>([from]);
    const queue: { id: string; path: string[] }[] = [{ id: from, path: [from] }];

    while (queue.length > 0) {
      const current = queue.shift()!;

      // Get all connected entities (outgoing and incoming relations)
      const neighbors = this.db.prepare(`
        SELECT source, target FROM relations
        WHERE source = ? OR target = ?
      `).all(current.id) as { source: string; target: string }[];

      for (const { source, target } of neighbors) {
        const neighbor = source === current.id ? target : source;

        if (neighbor === to) {
          return [...current.path, neighbor];
        }

        if (!visited.has(neighbor)) {
          visited.add(neighbor);
          queue.push({ id: neighbor, path: [...current.path, neighbor] });
        }
      }

      // Check hop limit
      if (current.path.length >= maxHops) {
        continue;
      }
    }

    return []; // No path found
  }

  // -------------------------------------------------------------------------
  // search — full-text search across entity name, type, properties, observations
  // -------------------------------------------------------------------------

  search(text: string): Entity[] {
    const pattern = `%${text.toLowerCase()}%`;

    const rows = this.db.prepare(`
      SELECT * FROM entities
      WHERE scope = 'global' OR scope = ?
      ORDER BY updated_at DESC
    `).all(this.scope) as DbEntityRow[];

    return rows
      .map((row) => this.rowToEntity(row))
      .filter((entity) => {
        const haystack = [
          entity.name,
          entity.type,
          ...Object.values(entity.properties).map(String),
          ...entity.observations,
        ]
          .join(' ')
          .toLowerCase();

        return haystack.includes(text.toLowerCase());
      });
  }

  // -------------------------------------------------------------------------
  // getVersion — bitemporal time-travel query
  // -------------------------------------------------------------------------

  getVersion(entityId: string, asOf: number): Entity | null {
    // Find the version that was current at `asOf` in both valid time and transaction time
    const row = this.db.prepare(`
      SELECT snapshot FROM entity_versions
      WHERE entity_id = ?
        AND valid_start <= ?
        AND (valid_end IS NULL OR valid_end > ?)
        AND transaction_start <= ?
        AND (transaction_end IS NULL OR transaction_end > ?)
      ORDER BY transaction_start DESC
      LIMIT 1
    `).get(entityId, asOf, asOf, asOf, asOf) as { snapshot: string } | undefined;

    if (!row) {
      return null;
    }

    return JSON.parse(row.snapshot) as Entity;
  }

  // -------------------------------------------------------------------------
  // getRelation — fetch a single relation by id
  // -------------------------------------------------------------------------

  getRelation(id: string): Relation | null {
    const row = this.db.prepare(`SELECT * FROM relations WHERE id = ?`).get(id) as DbRelationRow | undefined;
    return row ? this.rowToRelation(row) : null;
  }

  // -------------------------------------------------------------------------
  // getRelations — get all relations for an entity (as source or target)
  // -------------------------------------------------------------------------

  getRelations(entityId: string): Relation[] {
    const rows = this.db.prepare(`
      SELECT * FROM relations
      WHERE source = ? OR target = ?
    `).all(entityId, entityId) as DbRelationRow[];

    return rows.map(this.rowToRelation);
  }

  // -------------------------------------------------------------------------
  // deleteEntity — remove entity and all its relations
  // -------------------------------------------------------------------------

  deleteEntity(id: string): void {
    // Close out versions
    const now = Date.now();
    this.db.prepare(`
      UPDATE entity_versions
      SET valid_end = ?, transaction_end = ?
      WHERE entity_id = ? AND valid_end IS NULL AND transaction_end IS NULL
    `).run(now, now, id);

    this.db.prepare(`DELETE FROM entities WHERE id = ?`).run(id);
    log.debug('Entity deleted', { id });
  }

  // -------------------------------------------------------------------------
  // updateEntity — update entity properties, records a new version
  // -------------------------------------------------------------------------

  updateEntity(id: string, updates: Partial<Pick<Entity, 'name' | 'properties' | 'observations'>>): void {
    const now = Date.now();

    const row = this.db.prepare(`SELECT * FROM entities WHERE id = ?`).get(id) as DbEntityRow | undefined;
    if (!row) {
      log.warn('updateEntity: entity not found', { id });
      return;
    }

    const current = this.rowToEntity(row);
    const updated: Entity = {
      ...current,
      name: updates.name ?? current.name,
      properties: updates.properties ?? current.properties,
      observations: updates.observations ?? current.observations,
      updatedAt: now,
    };

    this.db.prepare(`
      UPDATE entities
      SET name = ?, properties = ?, observations = ?, updated_at = ?
      WHERE id = ?
    `).run(updated.name, JSON.stringify(updated.properties), JSON.stringify(updated.observations), now, id);

    // Close previous version's valid time
    this.db.prepare(`
      UPDATE entity_versions
      SET valid_end = ?
      WHERE entity_id = ? AND valid_end IS NULL
    `).run(now, id);

    // Record new version
    this.recordVersion(id, updated, now, now);

    log.debug('Entity updated', { id });
  }

  // -------------------------------------------------------------------------
  // Internal helpers
  // -------------------------------------------------------------------------

  private recordVersion(entityId: string, entity: Entity, validStart: number, txStart: number): void {
    const id = generateId();

    this.db.prepare(`
      INSERT INTO entity_versions (id, entity_id, valid_start, transaction_start, snapshot)
      VALUES (?, ?, ?, ?, ?)
    `).run(id, entityId, validStart, txStart, JSON.stringify(entity));
  }

  private rowToEntity(row: DbEntityRow): Entity {
    return {
      id: row.id,
      type: row.type,
      name: row.name,
      properties: JSON.parse(row.properties) as Record<string, unknown>,
      observations: JSON.parse(row.observations) as string[],
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    };
  }

  private rowToRelation(row: DbRelationRow): Relation {
    return {
      id: row.id,
      source: row.source,
      target: row.target,
      type: row.type,
      properties: JSON.parse(row.properties) as Record<string, unknown>,
    };
  }

  /** Close the database connection */
  close(): void {
    this.db.close();
  }

  /** Get stats */
  stats(): { entities: number; relations: number; versions: number } {
    const entities = (this.db.prepare(`SELECT COUNT(*) as count FROM entities`).get() as { count: number }).count;
    const relations = (this.db.prepare(`SELECT COUNT(*) as count FROM relations`).get() as { count: number }).count;
    const versions = (this.db.prepare(`SELECT COUNT(*) as count FROM entity_versions`).get() as { count: number }).count;

    return { entities, relations, versions };
  }
}

// ---------------------------------------------------------------------------
// Internal row types (SQLite)
// ---------------------------------------------------------------------------

interface DbEntityRow {
  id: string;
  type: string;
  name: string;
  properties: string;
  observations: string;
  created_at: number;
  updated_at: number;
  scope: string;
  scope_id: string | null;
}

interface DbRelationRow {
  id: string;
  source: string;
  target: string;
  type: string;
  properties: string;
  created_at: number;
  scope: string;
  scope_id: string | null;
}
