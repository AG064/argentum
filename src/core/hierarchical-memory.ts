/**
 * Hierarchical Memory System (MemOS Pattern)
 *
 * Three-tier memory architecture:
 *   - SHORT: Current task context, auto-evicted at 50 entries
 *   - MID:   Recent sessions, preserved longer, auto-promotes accessed >3 times
 *   - LONG:  Important facts (importance >0.7), never auto-evicted
 *
 * Promotion: entry with accessCount >5 AND importance >0.5 moves up a tier
 * Demotion: long-term entries with importance <0.3 move to mid-tier
 * Prune:    removes oldest short/mid entries beyond tier limits
 * Compact:  merges entries with >0.85 similarity in the same tier
 */

import Database from 'better-sqlite3';
import { existsSync, mkdirSync } from 'fs';
import { dirname, resolve } from 'path';
import { createHash } from 'crypto';

import { featureLogger } from './logger';

const log = featureLogger('hierarchical-memory');

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export enum MemoryTier {
  SHORT = 'short',
  MID = 'mid',
  LONG = 'long',
}

export interface MemoryEntry {
  id: string;
  tier: MemoryTier;
  content: string;
  importance: number; // 0-1
  accessCount: number;
  createdAt: number;
  lastAccessed: number;
  tags: string[];
}

export interface HierarchicalMemory {
  store(entry: MemoryEntry): MemoryEntry;
  retrieve(query: string, maxResults: number): MemoryEntry[];
  promote(id: string): void;
  demote(id: string): void;
  prune(): void;
  compact(): void;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SHORT_MAX = 50;
const MID_MAX = 500;

const SHORT_CREATE = MemoryTier.SHORT;
const MID_CREATE = MemoryTier.MID;
const LONG_CREATE = MemoryTier.LONG;

const PROMOTE_ACCESS_COUNT = 5;
const PROMOTE_MIN_IMPORTANCE = 0.5;

const AUTO_PROMOTE_ACCESS = 3; // entries accessed >3 times → mid tier auto-consider

const LONG_MIN_IMPORTANCE = 0.7;

const DEMOTE_IMPORTANCE = 0.3; // long-term entries below this → demote to mid

const COMPACT_SIMILARITY = 0.85;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function generateId(): string {
  return createHash('sha256').update(Date.now().toString() + Math.random().toString()).digest('hex').slice(0, 16);
}

function serializeTags(tags: string[]): string {
  return JSON.stringify(tags);
}

function deserializeTags(raw: string): string[] {
  try {
    return JSON.parse(raw) as string[];
  } catch {
    return [];
  }
}

// Simple content similarity using trigram overlap
function contentSimilarity(a: string, b: string): number {
  const normalize = (s: string): string[] =>
    s.toLowerCase().split(/\s+/).filter((w) => w.length > 2);
  const wordsA = normalize(a);
  const wordsB = normalize(b);
  if (wordsA.length === 0 || wordsB.length === 0) return 0;

  const setB = new Set(wordsB);
  const intersection = wordsA.filter((w) => setB.has(w)).length;
  const union = new Set([...wordsA, ...wordsB]).size;
  return union > 0 ? intersection / union : 0;
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

export class HierarchicalMemoryStore {
  private db: Database.Database;
  private flushPending = false;

  constructor(dbPath?: string) {
    const resolved = resolve(dbPath ?? './data/hierarchical-memory.db');
    const dir = dirname(resolved);
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }

    this.db = new Database(resolved);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('foreign_keys = ON');
    this.initSchema();
  }

  private initSchema(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS memory_entries (
        id          TEXT PRIMARY KEY,
        tier        TEXT NOT NULL DEFAULT 'short',
        content     TEXT NOT NULL,
        importance  REAL NOT NULL DEFAULT 0.5,
        access_count INTEGER NOT NULL DEFAULT 1,
        created_at  INTEGER NOT NULL,
        last_accessed INTEGER NOT NULL,
        tags        TEXT NOT NULL DEFAULT '[]'
      );

      CREATE INDEX IF NOT EXISTS idx_tier ON memory_entries(tier);
      CREATE INDEX IF NOT EXISTS idx_created ON memory_entries(created_at);
      CREATE INDEX IF NOT EXISTS idx_access_count ON memory_entries(access_count);
      CREATE INDEX IF NOT EXISTS idx_importance ON memory_entries(importance);
    `);
  }

  // -------------------------------------------------------------------------
  // store
  // -------------------------------------------------------------------------

  store(entry: MemoryEntry): MemoryEntry { // line 155
    const now = Date.now();
    const id = entry.id || generateId();

    const insert = this.db.prepare(`
      INSERT OR REPLACE INTO memory_entries
        (id, tier, content, importance, access_count, created_at, last_accessed, tags)
      VALUES
        (@id, @tier, @content, @importance, @accessCount, @createdAt, @lastAccessed, @tags)
    `);

    const row = {
      id,
      tier: entry.tier,
      content: entry.content,
      importance: entry.importance,
      accessCount: entry.accessCount,
      createdAt: entry.createdAt,
      lastAccessed: entry.lastAccessed,
      tags: serializeTags(entry.tags),
    };

    insert.run(row);
    log.debug('Stored entry', { id: row.id, tier: row.tier });

    // Enforce short-term cap
    this.enforceShortCap();

    // Auto-promote from short to mid if accessed >3 times
    if (entry.tier === MemoryTier.SHORT && entry.accessCount > AUTO_PROMOTE_ACCESS) {
      this.promoteInternal(row.id);
    }

    // Auto-promote to long if importance >0.7
    if (entry.importance >= LONG_MIN_IMPORTANCE && entry.tier !== MemoryTier.LONG) {
      this.promoteInternal(row.id);
    }

    return { ...entry, id };
  }

  // -------------------------------------------------------------------------
  // retrieve — simple keyword match + recency scoring
  // -------------------------------------------------------------------------

  retrieve(query: string, maxResults: number): MemoryEntry[] {
    if (!query.trim()) {
      // Return most recently accessed entries across all tiers
      const stmt = this.db.prepare(`
        SELECT * FROM memory_entries
        ORDER BY last_accessed DESC
        LIMIT ?
      `);
      const rows = (stmt.all(maxResults) as DbRow[]);
      return rows.map(this.rowToEntry);
    }

    const keywords = query.toLowerCase().split(/\s+/).filter((w) => w.length > 2);

    const allStmt = this.db.prepare(`SELECT * FROM memory_entries ORDER BY last_accessed DESC`);
    const rows = (allStmt.all() as DbRow[]);

    const scored = rows
      .map((row) => {
        const entry = this.rowToEntry(row);
        const contentLower = entry.content.toLowerCase();
        let score = 0;

        for (const kw of keywords) {
          if (contentLower.includes(kw)) score += 1;
        }

        // Boost by access count and importance
        score += entry.accessCount * 0.1;
        score += entry.importance * 0.5;

        return { entry, score };
      })
      .filter((s) => s.score > 0)
      .sort((a, b) => b.score - a.score);

    // If nothing matched keywords, fall back to recency
    if (scored.length === 0) {
      return rows.map((r) => this.rowToEntry(r)).slice(0, maxResults);
    }

    return scored.slice(0, maxResults).map((s) => s.entry);
  }

  // -------------------------------------------------------------------------
  // promote — move entry up one tier
  // -------------------------------------------------------------------------

  promote(id: string): void {
    this.promoteInternal(id);
  }

  private promoteInternal(id: string): void {
    const row = this.db.prepare(`SELECT * FROM memory_entries WHERE id = ?`).get(id) as DbRow | undefined;
    if (!row) {
      log.warn('Promote: entry not found', { id });
      return;
    }

    const currentTier = row.tier as MemoryTier;
    let newTier: MemoryTier;

    if (currentTier === MemoryTier.SHORT) {
      newTier = MemoryTier.MID;
    } else if (currentTier === MemoryTier.MID) {
      newTier = MemoryTier.LONG;
    } else {
      // Already at LONG, nothing to promote
      return;
    }

    this.db.prepare(`UPDATE memory_entries SET tier = ? WHERE id = ?`).run(newTier, id);
    log.debug('Promoted entry', { id, from: currentTier, to: newTier });
  }

  // -------------------------------------------------------------------------
  // demote — move entry down one tier
  // -------------------------------------------------------------------------

  demote(id: string): void {
    const row = this.db.prepare(`SELECT * FROM memory_entries WHERE id = ?`).get(id) as DbRow | undefined;
    if (!row) {
      log.warn('Demote: entry not found', { id });
      return;
    }

    const currentTier = row.tier as MemoryTier;
    let newTier: MemoryTier;

    if (currentTier === MemoryTier.LONG) {
      newTier = MemoryTier.MID;
    } else if (currentTier === MemoryTier.MID) {
      newTier = MemoryTier.SHORT;
    } else {
      // Already at SHORT, nothing to demote
      return;
    }

    this.db.prepare(`UPDATE memory_entries SET tier = ? WHERE id = ?`).run(newTier, id);
    log.debug('Demoted entry', { id, from: currentTier, to: newTier });
  }

  // -------------------------------------------------------------------------
  // prune — remove old entries beyond tier limits
  // -------------------------------------------------------------------------

  prune(): void {
    // Remove oldest SHORT entries beyond 50
    this.db.prepare(`
      DELETE FROM memory_entries WHERE id IN (
        SELECT id FROM memory_entries
        WHERE tier = ?
        ORDER BY created_at ASC
        LIMIT MAX(0, (SELECT COUNT(*) FROM memory_entries WHERE tier = ?) - ?)
      )
    `).run(MemoryTier.SHORT, MemoryTier.SHORT, SHORT_MAX);

    // Remove oldest MID entries beyond 500
    this.db.prepare(`
      DELETE FROM memory_entries WHERE id IN (
        SELECT id FROM memory_entries
        WHERE tier = ?
        ORDER BY created_at ASC
        LIMIT MAX(0, (SELECT COUNT(*) FROM memory_entries WHERE tier = ?) - ?)
      )
    `).run(MemoryTier.MID, MemoryTier.MID, MID_MAX);

    // Demote long-term entries that dropped in importance
    const demoteCandidates = this.db.prepare(`
      SELECT id FROM memory_entries
      WHERE tier = ? AND importance < ?
    `).all(MemoryTier.LONG, DEMOTE_IMPORTANCE) as { id: string }[];

    for (const { id } of demoteCandidates) {
      this.demote(id);
    }

    log.info('Prune complete', {
      shortCount: this.countTier(MemoryTier.SHORT),
      midCount: this.countTier(MemoryTier.MID),
      longCount: this.countTier(MemoryTier.LONG),
    });
  }

  // -------------------------------------------------------------------------
  // compact — merge similar entries in the same tier
  // -------------------------------------------------------------------------

  compact(): void {
    for (const tier of [MemoryTier.SHORT, MemoryTier.MID, MemoryTier.LONG] as MemoryTier[]) {
      const rows = this.db.prepare(
        `SELECT * FROM memory_entries WHERE tier = ? ORDER BY created_at DESC`,
      ).all(tier) as DbRow[];

      const merged = new Set<string>();

      for (let i = 0; i < rows.length; i++) {
        if (merged.has(rows[i]!.id)) continue;

        for (let j = i + 1; j < rows.length; j++) {
          if (merged.has(rows[j]!.id)) continue;

          const sim = contentSimilarity(rows[i]!.content, rows[j]!.content);
          if (sim >= COMPACT_SIMILARITY) {
            // Keep the newer (higher access count + importance), delete the older
            const keep = rows[i]!;
            const remove = rows[j]!;

            const updatedAccessCount = keep.access_count + remove.access_count;
            const updatedImportance = Math.max(keep.importance, remove.importance);
            const updatedTags = Array.from(new Set([
              ...deserializeTags(keep.tags),
              ...deserializeTags(remove.tags),
            ]));

            this.db.prepare(`
              UPDATE memory_entries
              SET access_count = ?,
                  importance = ?,
                  tags = ?,
                  last_accessed = ?
              WHERE id = ?
            `).run(updatedAccessCount, updatedImportance, serializeTags(updatedTags), Date.now(), keep.id);

            this.db.prepare(`DELETE FROM memory_entries WHERE id = ?`).run(remove.id);

            merged.add(remove.id);
            log.debug('Compacted entries', {
              keepId: keep.id,
              removedId: remove.id,
              similarity: sim.toFixed(2),
            });
          }
        }
      }
    }
  }

  // -------------------------------------------------------------------------
  // Internal helpers
  // -------------------------------------------------------------------------

  private enforceShortCap(): void {
    const countStmt = this.db.prepare(`SELECT COUNT(*) as count FROM memory_entries WHERE tier = ?`);
    const { count } = countStmt.get(MemoryTier.SHORT) as { count: number };

    if (count > SHORT_MAX) {
      const excess = count - SHORT_MAX;
      this.db.prepare(`
        DELETE FROM memory_entries WHERE id IN (
          SELECT id FROM memory_entries
          WHERE tier = ?
          ORDER BY created_at ASC
          LIMIT ?
        )
      `).run(MemoryTier.SHORT, excess);
    }
  }

  private countTier(tier: MemoryTier): number {
    const { count } = this.db.prepare(
      `SELECT COUNT(*) as count FROM memory_entries WHERE tier = ?`,
    ).get(tier) as { count: number };
    return count;
  }

  private rowToEntry(row: DbRow): MemoryEntry {
    return {
      id: row.id,
      tier: row.tier as MemoryTier,
      content: row.content,
      importance: row.importance,
      accessCount: row.access_count,
      createdAt: row.created_at,
      lastAccessed: row.last_accessed,
      tags: deserializeTags(row.tags),
    };
  }

  /** Close the database connection */
  close(): void {
    this.db.close();
  }

  /** Get stats for all tiers */
  stats(): { tier: MemoryTier; count: number }[] {
    return [
      { tier: MemoryTier.SHORT, count: this.countTier(MemoryTier.SHORT) },
      { tier: MemoryTier.MID, count: this.countTier(MemoryTier.MID) },
      { tier: MemoryTier.LONG, count: this.countTier(MemoryTier.LONG) },
    ];
  }
}

// ---------------------------------------------------------------------------
// Internal row type (SQLite)
// ---------------------------------------------------------------------------

interface DbRow {
  id: string;
  tier: string;
  content: string;
  importance: number;
  access_count: number;
  created_at: number;
  last_accessed: number;
  tags: string;
}
