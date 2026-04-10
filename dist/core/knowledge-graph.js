"use strict";
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
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.KnowledgeGraphMemory = void 0;
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
const logger_1 = require("./logger");
const id_1 = require("../utils/id");
const log = (0, logger_1.featureLogger)('knowledge-graph');
// ---------------------------------------------------------------------------
// KnowledgeGraphMemory
// ---------------------------------------------------------------------------
class KnowledgeGraphMemory {
    db;
    scope = 'global';
    scopeId = null;
    constructor(dbPath) {
        const resolved = (0, path_1.resolve)(dbPath ?? './data/knowledge-graph.db');
        const dir = (0, path_1.dirname)(resolved);
        if (!(0, fs_1.existsSync)(dir)) {
            (0, fs_1.mkdirSync)(dir, { recursive: true });
        }
        this.db = new better_sqlite3_1.default(resolved);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('foreign_keys = ON');
        this.initSchema();
    }
    // -------------------------------------------------------------------------
    // Schema
    // -------------------------------------------------------------------------
    initSchema() {
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
    setScope(scope, scopeId) {
        this.scope = scope;
        this.scopeId = scopeId ?? null;
        log.debug('Scope set', { scope, scopeId: this.scopeId });
    }
    getScope() {
        return this.scope;
    }
    // -------------------------------------------------------------------------
    // addEntity
    // -------------------------------------------------------------------------
    addEntity(entity) {
        const now = Date.now();
        const id = (0, id_1.generateId)();
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
        this.recordVersion(id, entity, now, now);
        log.debug('Entity added', { id, type: entity.type, name: entity.name });
        return id;
    }
    // -------------------------------------------------------------------------
    // addRelation
    // -------------------------------------------------------------------------
    addRelation(relation) {
        const now = Date.now();
        const id = (0, id_1.generateId)();
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
    addObservation(entityId, observation) {
        const now = Date.now();
        const row = this.db.prepare(`SELECT observations FROM entities WHERE id = ?`).get(entityId);
        if (!row) {
            log.warn('addObservation: entity not found', { entityId });
            return;
        }
        const observations = JSON.parse(row.observations);
        observations.push(observation);
        this.db.prepare(`UPDATE entities SET observations = ?, updated_at = ? WHERE id = ?`).run(JSON.stringify(observations), now, entityId);
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
    query(entities, relations) {
        let stmt;
        if (entities && entities.length > 0) {
            const placeholders = entities.map(() => '?').join(', ');
            const rows = this.db.prepare(`
        SELECT * FROM entities
        WHERE type IN (${placeholders})
          AND (scope = 'global' OR scope = ?)
          AND (scope != 'global' OR scope_id IS NULL OR scope_id = ?)
        ORDER BY created_at DESC
      `).all(...entities, this.scope, this.scopeId ?? '');
            return rows.map(this.rowToEntity);
        }
        const rows = this.db.prepare(`
      SELECT * FROM entities
      WHERE scope = 'global' OR scope = ?
      ORDER BY created_at DESC
    `).all(this.scope);
        return rows.map(this.rowToEntity);
    }
    // -------------------------------------------------------------------------
    // getEntity — fetch a single entity by id
    // -------------------------------------------------------------------------
    getEntity(id) {
        const row = this.db.prepare(`SELECT * FROM entities WHERE id = ?`).get(id);
        return row ? this.rowToEntity(row) : null;
    }
    // -------------------------------------------------------------------------
    // findPath — BFS graph traversal between two entity ids
    // -------------------------------------------------------------------------
    findPath(from, to, maxHops = 4) {
        if (from === to)
            return [from];
        const visited = new Set([from]);
        const queue = [{ id: from, path: [from] }];
        while (queue.length > 0) {
            const current = queue.shift();
            // Get all connected entities (outgoing and incoming relations)
            const neighbors = this.db.prepare(`
        SELECT source, target FROM relations
        WHERE source = ? OR target = ?
      `).all(current.id);
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
    search(text) {
        const pattern = `%${text.toLowerCase()}%`;
        const rows = this.db.prepare(`
      SELECT * FROM entities
      WHERE scope = 'global' OR scope = ?
      ORDER BY updated_at DESC
    `).all(this.scope);
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
    getVersion(entityId, asOf) {
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
    `).get(entityId, asOf, asOf, asOf, asOf);
        if (!row) {
            return null;
        }
        return JSON.parse(row.snapshot);
    }
    // -------------------------------------------------------------------------
    // getRelation — fetch a single relation by id
    // -------------------------------------------------------------------------
    getRelation(id) {
        const row = this.db.prepare(`SELECT * FROM relations WHERE id = ?`).get(id);
        return row ? this.rowToRelation(row) : null;
    }
    // -------------------------------------------------------------------------
    // getRelations — get all relations for an entity (as source or target)
    // -------------------------------------------------------------------------
    getRelations(entityId) {
        const rows = this.db.prepare(`
      SELECT * FROM relations
      WHERE source = ? OR target = ?
    `).all(entityId, entityId);
        return rows.map(this.rowToRelation);
    }
    // -------------------------------------------------------------------------
    // deleteEntity — remove entity and all its relations
    // -------------------------------------------------------------------------
    deleteEntity(id) {
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
    updateEntity(id, updates) {
        const now = Date.now();
        const row = this.db.prepare(`SELECT * FROM entities WHERE id = ?`).get(id);
        if (!row) {
            log.warn('updateEntity: entity not found', { id });
            return;
        }
        const current = this.rowToEntity(row);
        const updated = {
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
    recordVersion(entityId, entity, validStart, txStart) {
        const id = (0, id_1.generateId)();
        this.db.prepare(`
      INSERT INTO entity_versions (id, entity_id, valid_start, transaction_start, snapshot)
      VALUES (?, ?, ?, ?, ?)
    `).run(id, entityId, validStart, txStart, JSON.stringify(entity));
    }
    rowToEntity(row) {
        return {
            id: row.id,
            type: row.type,
            name: row.name,
            properties: JSON.parse(row.properties),
            observations: JSON.parse(row.observations),
            createdAt: row.created_at,
            updatedAt: row.updated_at,
        };
    }
    rowToRelation(row) {
        return {
            id: row.id,
            source: row.source,
            target: row.target,
            type: row.type,
            properties: JSON.parse(row.properties),
        };
    }
    /** Close the database connection */
    close() {
        this.db.close();
    }
    /** Get stats */
    stats() {
        const entities = this.db.prepare(`SELECT COUNT(*) as count FROM entities`).get().count;
        const relations = this.db.prepare(`SELECT COUNT(*) as count FROM relations`).get().count;
        const versions = this.db.prepare(`SELECT COUNT(*) as count FROM entity_versions`).get().count;
        return { entities, relations, versions };
    }
}
exports.KnowledgeGraphMemory = KnowledgeGraphMemory;
//# sourceMappingURL=knowledge-graph.js.map