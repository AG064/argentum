"use strict";
/**
 * Knowledge Graph Feature
 *
 * Entity-relationship knowledge graph with file import (Markdown, JSON),
 * graph export, BFS pathfinding, and SQLite + in-memory backends.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
function parseMarkdown(content) {
    const entities = [];
    const lines = content.split('\n');
    let current = null;
    for (const line of lines) {
        // ## Entity Name (type: Person)
        const headerMatch = line.match(/^##\s+(.+?)(?:\s*\((.+?)\))?$/);
        if (headerMatch) {
            if (current)
                entities.push(current);
            const typeMatch = headerMatch[2]?.match(/type:\s*(.+)/i);
            current = {
                name: headerMatch[1].trim(),
                type: typeMatch?.[1]?.trim() ?? 'unknown',
                properties: {},
                relations: [],
            };
            continue;
        }
        if (!current)
            continue;
        // - **key**: value  (property)
        const propMatch = line.match(/^\s*-\s*\*\*(.+?)\*\*:\s*(.+)$/);
        if (propMatch) {
            current.properties[propMatch[1].trim()] = propMatch[2].trim();
            continue;
        }
        // - relates_to: Other Entity (weight: 0.8)
        const relMatch = line.match(/^\s*-\s*(.+?):\s*(.+?)(?:\s*\((.+?)\))?$/);
        if (relMatch && !relMatch[1].startsWith('**')) {
            const weightMatch = relMatch[3]?.match(/weight:\s*([\d.]+)/);
            current.relations.push({
                type: relMatch[1].trim().replace(/\s+/g, '_'),
                target: relMatch[2].trim(),
                weight: weightMatch ? parseFloat(weightMatch[1]) : 1.0,
            });
        }
    }
    if (current)
        entities.push(current);
    return entities;
}
// ─── SQLite Backend ──────────────────────────────────────────────────────────
class SQLiteGraphBackend {
    path;
    db;
    constructor(path) {
        this.path = path;
    }
    async init() {
        (0, fs_1.mkdirSync)((0, path_1.dirname)(this.path), { recursive: true });
        this.db = new better_sqlite3_1.default(this.path);
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
    async close() {
        this.db?.close();
    }
    async addEntity(data) {
        const id = `ent_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const now = Date.now();
        this.db
            .prepare('INSERT INTO entities (id, type, name, properties, tags, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)')
            .run(id, data.type, data.name, JSON.stringify(data.properties), JSON.stringify(data.tags ?? []), now, now);
        return { id, ...data, tags: data.tags ?? [], createdAt: now, updatedAt: now };
    }
    async getEntity(id) {
        const row = this.db.prepare('SELECT * FROM entities WHERE id = ?').get(id);
        if (!row)
            return null;
        return {
            id: row['id'],
            type: row['type'],
            name: row['name'],
            properties: JSON.parse(row['properties']),
            tags: JSON.parse(row['tags'] ?? '[]'),
            createdAt: row['created_at'],
            updatedAt: row['updated_at'],
        };
    }
    async updateEntity(id, updates) {
        const existing = await this.getEntity(id);
        if (!existing)
            throw new Error(`Entity not found: ${id}`);
        const now = Date.now();
        const merged = { ...existing, ...updates, updatedAt: now };
        this.db
            .prepare('UPDATE entities SET name=?, properties=?, tags=?, updated_at=? WHERE id=?')
            .run(merged.name, JSON.stringify(merged.properties), JSON.stringify(merged.tags), now, id);
        return merged;
    }
    async deleteEntity(id) {
        this.db.prepare('DELETE FROM relationships WHERE source_id = ? OR target_id = ?').run(id, id);
        this.db.prepare('DELETE FROM entities WHERE id = ?').run(id);
    }
    async addRelationship(data) {
        const id = `rel_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const now = Date.now();
        this.db
            .prepare('INSERT INTO relationships (id, source_id, target_id, type, properties, weight, created_at) VALUES (?,?,?,?,?,?,?)')
            .run(id, data.sourceId, data.targetId, data.type, JSON.stringify(data.properties), data.weight, now);
        return { id, ...data, createdAt: now };
    }
    async getRelationships(entityId, type) {
        let query = 'SELECT * FROM relationships WHERE source_id = ? OR target_id = ?';
        const params = [entityId, entityId];
        if (type) {
            query += ' AND type = ?';
            params.push(type);
        }
        const rows = this.db.prepare(query).all(...params);
        return rows.map(this.rowToRel);
    }
    async getAllRelationships() {
        return this.db.prepare('SELECT * FROM relationships').all().map(this.rowToRel);
    }
    async findEntities(query) {
        let sql = 'SELECT * FROM entities WHERE 1=1';
        const params = [];
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
        return this.db.prepare(sql).all(...params).map(this.rowToEntity);
    }
    async getAllEntities() {
        return this.db.prepare('SELECT * FROM entities').all().map(this.rowToEntity);
    }
    async findPaths(sourceId, targetId, maxDepth = 5) {
        const visited = new Set();
        const queue = [{ id: sourceId, path: [sourceId] }];
        const paths = [];
        while (queue.length > 0 && paths.length < 10) {
            const { id, path } = queue.shift();
            if (id === targetId) {
                const entities = await Promise.all(path.map((pid) => this.getEntity(pid)));
                paths.push(entities.filter((e) => e !== null));
                continue;
            }
            if (path.length >= maxDepth || visited.has(id))
                continue;
            visited.add(id);
            const rels = await this.getRelationships(id);
            for (const rel of rels) {
                const nextId = rel.sourceId === id ? rel.targetId : rel.sourceId;
                if (!visited.has(nextId))
                    queue.push({ id: nextId, path: [...path, nextId] });
            }
        }
        return paths;
    }
    async getStats() {
        const entities = this.db.prepare('SELECT COUNT(*) as c FROM entities').get()
            .c;
        const relationships = this.db.prepare('SELECT COUNT(*) as c FROM relationships').get().c;
        return { entities, relationships };
    }
    async clear() {
        this.db.exec('DELETE FROM relationships; DELETE FROM entities;');
    }
    rowToEntity(row) {
        return {
            id: row['id'],
            type: row['type'],
            name: row['name'],
            properties: JSON.parse(row['properties']),
            tags: JSON.parse(row['tags'] ?? '[]'),
            createdAt: row['created_at'],
            updatedAt: row['updated_at'],
        };
    }
    rowToRel(row) {
        return {
            id: row['id'],
            sourceId: row['source_id'],
            targetId: row['target_id'],
            type: row['type'],
            properties: JSON.parse(row['properties']),
            weight: row['weight'],
            createdAt: row['created_at'],
        };
    }
}
// ─── In-Memory Backend ───────────────────────────────────────────────────────
class MemoryGraphBackend {
    entities = new Map();
    relationships = new Map();
    async init() { }
    async close() { }
    async addEntity(data) {
        const id = `ent_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const now = Date.now();
        const entity = { id, ...data, tags: data.tags ?? [], createdAt: now, updatedAt: now };
        this.entities.set(id, entity);
        return entity;
    }
    async getEntity(id) {
        return this.entities.get(id) ?? null;
    }
    async updateEntity(id, updates) {
        const existing = this.entities.get(id);
        if (!existing)
            throw new Error(`Entity not found: ${id}`);
        const updated = { ...existing, ...updates, updatedAt: Date.now() };
        this.entities.set(id, updated);
        return updated;
    }
    async deleteEntity(id) {
        this.entities.delete(id);
        for (const [rid, rel] of this.relationships) {
            if (rel.sourceId === id || rel.targetId === id)
                this.relationships.delete(rid);
        }
    }
    async addRelationship(data) {
        const id = `rel_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const rel = { id, ...data, createdAt: Date.now() };
        this.relationships.set(id, rel);
        return rel;
    }
    async getRelationships(entityId, type) {
        return Array.from(this.relationships.values()).filter((r) => (r.sourceId === entityId || r.targetId === entityId) && (!type || r.type === type));
    }
    async getAllRelationships() {
        return Array.from(this.relationships.values());
    }
    async getAllEntities() {
        return Array.from(this.entities.values());
    }
    async findEntities(query) {
        return Array.from(this.entities.values()).filter((e) => {
            if (query.type && e.type !== query.type)
                return false;
            if (query.name && !e.name.toLowerCase().includes(query.name.toLowerCase()))
                return false;
            if (query.tags?.length && !query.tags.some((t) => e.tags.includes(t)))
                return false;
            return true;
        });
    }
    async findPaths(sourceId, targetId, maxDepth = 5) {
        const visited = new Set();
        const queue = [{ id: sourceId, path: [sourceId] }];
        const paths = [];
        while (queue.length > 0 && paths.length < 10) {
            const { id, path } = queue.shift();
            if (id === targetId) {
                paths.push(path.map((pid) => this.entities.get(pid)).filter(Boolean));
                continue;
            }
            if (path.length >= maxDepth || visited.has(id))
                continue;
            visited.add(id);
            for (const rel of await this.getRelationships(id)) {
                const nextId = rel.sourceId === id ? rel.targetId : rel.sourceId;
                if (!visited.has(nextId))
                    queue.push({ id: nextId, path: [...path, nextId] });
            }
        }
        return paths;
    }
    async getStats() {
        return { entities: this.entities.size, relationships: this.relationships.size };
    }
    async clear() {
        this.entities.clear();
        this.relationships.clear();
    }
}
// ─── Main Feature ────────────────────────────────────────────────────────────
/**
 * Knowledge Graph feature — entity-relationship graph with file import/export.
 */
class KnowledgeGraphFeature {
    meta = {
        name: 'knowledge-graph',
        version: '0.2.0',
        description: 'Knowledge graph with Markdown/JSON import, export, pathfinding',
        dependencies: [],
    };
    config = {
        enabled: false,
        backend: 'sqlite',
        path: './data/knowledge.db',
        importDir: './data/knowledge/imports',
        exportDir: './data/knowledge/exports',
    };
    ctx;
    backend = null;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        this.backend =
            this.config.backend === 'sqlite'
                ? new SQLiteGraphBackend(this.config.path)
                : new MemoryGraphBackend();
        await this.backend.init();
        this.ctx.logger.info('Knowledge Graph active', { backend: this.config.backend });
    }
    async stop() {
        await this.backend?.close();
        this.backend = null;
    }
    async healthCheck() {
        if (!this.backend)
            return { healthy: false, message: 'Not initialized' };
        const stats = await this.backend.getStats();
        return { healthy: true, details: stats };
    }
    // ─── CRUD ─────────────────────────────────────────────────────────────────
    async addEntity(type, name, properties = {}, tags = []) {
        if (!this.backend)
            throw new Error('Backend not initialized');
        return this.backend.addEntity({ type, name, properties, tags });
    }
    async getEntity(id) {
        if (!this.backend)
            throw new Error('Backend not initialized');
        return this.backend.getEntity(id);
    }
    async updateEntity(id, updates) {
        if (!this.backend)
            throw new Error('Backend not initialized');
        return this.backend.updateEntity(id, updates);
    }
    async deleteEntity(id) {
        if (!this.backend)
            throw new Error('Backend not initialized');
        return this.backend.deleteEntity(id);
    }
    async addRelationship(sourceId, targetId, type, properties = {}, weight = 1.0) {
        if (!this.backend)
            throw new Error('Backend not initialized');
        return this.backend.addRelationship({ sourceId, targetId, type, properties, weight });
    }
    async findEntities(query) {
        if (!this.backend)
            throw new Error('Backend not initialized');
        return this.backend.findEntities(query);
    }
    async findPaths(sourceId, targetId, maxDepth = 5) {
        if (!this.backend)
            throw new Error('Backend not initialized');
        return this.backend.findPaths(sourceId, targetId, maxDepth);
    }
    // ─── 3D Visualization Data ───────────────────────────────────────────────
    /**
     * Get graph data formatted for 3D visualization
     * Returns {nodes, links} format compatible with 3d-force-graph
     */
    async getGraph3DData() {
        if (!this.backend)
            throw new Error('Backend not initialized');
        const entities = await this.backend.getAllEntities();
        const relationships = await this.backend.getAllRelationships();
        return { entities, relationships };
    }
    // ─── Import ───────────────────────────────────────────────────────────────
    /** Import from Markdown file */
    async importFromMarkdown(filePath) {
        if (!this.backend)
            throw new Error('Backend not initialized');
        const content = (0, fs_1.readFileSync)(filePath, 'utf-8');
        const parsed = parseMarkdown(content);
        let entityCount = 0;
        let relCount = 0;
        const nameToId = new Map();
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
            if (!sourceId)
                continue;
            for (const rel of item.relations) {
                const targetId = nameToId.get(rel.target);
                if (!targetId)
                    continue;
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
    async importFromJson(filePath) {
        if (!this.backend)
            throw new Error('Backend not initialized');
        const data = JSON.parse((0, fs_1.readFileSync)(filePath, 'utf-8'));
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
    async importFromFile(filePath) {
        if (filePath.endsWith('.md') || filePath.endsWith('.markdown'))
            return this.importFromMarkdown(filePath);
        if (filePath.endsWith('.json'))
            return this.importFromJson(filePath);
        throw new Error(`Unsupported file format: ${filePath}`);
    }
    // ─── Export ───────────────────────────────────────────────────────────────
    /** Export as JSON graph data */
    async exportGraph() {
        if (!this.backend)
            throw new Error('Backend not initialized');
        const entities = await this.backend.getAllEntities();
        const relationships = await this.backend.getAllRelationships();
        const stats = await this.backend.getStats();
        return { entities, relationships, exportedAt: Date.now(), stats };
    }
    /** Export to JSON file */
    async exportToFile(filePath) {
        const data = await this.exportGraph();
        (0, fs_1.mkdirSync)((0, path_1.dirname)(filePath), { recursive: true });
        (0, fs_1.writeFileSync)(filePath, JSON.stringify(data, null, 2), 'utf-8');
        this.ctx.logger.info('Graph exported', { file: filePath, ...data.stats });
        return data;
    }
}
exports.default = new KnowledgeGraphFeature();
//# sourceMappingURL=index.js.map