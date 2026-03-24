/**
 * Vector Embeddings Module
 * 
 * Simple TF-IDF based text embeddings with cosine similarity search.
 * Stores embeddings in SQLite alongside existing entity data.
 * 
 * Features:
 * - TF-IDF vectorization (no external ML dependencies)
 * - Cosine similarity search
 * - Fallback to keyword matching when vectors unavailable
 * - Persistent storage in SQLite
 */

import Database from 'better-sqlite3';
import path from 'path';
import fs from 'fs';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface EmbeddingRecord {
  id: number;
  entity_id: string;
  text: string;
  vector: number[];  // TF-IDF vector
  created_at: number;
  updated_at: number;
}

export interface SimilarityResult {
  entity_id: string;
  text: string;
  similarity: number;
}

export interface SearchOptions {
  limit?: number;
  minSimilarity?: number;
  type?: string;  // Filter by entity type
}

// ─── TF-IDF Implementation ────────────────────────────────────────────────────

/**
 * Simple TF-IDF Vectorizer
 * 
 * TF(t,d) = count of term t in document d
 * IDF(t) = log(N / df(t)) where N = total docs, df(t) = docs containing t
 * TF-IDF(t,d) = TF(t,d) * IDF(t)
 */
export class TFIDFVectorizer {
  private vocabulary: Map<string, number> = new Map();
  private idf: Map<string, number> = new Map();
  private documentCount = 0;

  /**
   * Tokenize text into terms
   */
  private tokenize(text: string): string[] {
    return text
      .toLowerCase()
      .replace(/[^\w\s]/g, ' ')
      .split(/\s+/)
      .filter(term => term.length > 1);
  }

  /**
   * Compute term frequency for a document
   */
  private computeTF(terms: string[]): Map<string, number> {
    const tf = new Map<string, number>();
    for (const term of terms) {
      tf.set(term, (tf.get(term) || 0) + 1);
    }
    // Normalize by document length
    for (const [term, count] of tf) {
      tf.set(term, count / terms.length);
    }
    return tf;
  }

  /**
   * Fit the vectorizer on a corpus of documents
   */
  fit(documents: string[]): void {
    this.documentCount = documents.length;
    this.vocabulary.clear();
    this.idf.clear();

    // Build vocabulary and document frequencies
    const docFreq = new Map<string, number>();
    let vocabIndex = 0;

    for (const doc of documents) {
      const terms = this.tokenize(doc);
      const uniqueTerms = new Set(terms);

      for (const term of uniqueTerms) {
        if (!this.vocabulary.has(term)) {
          this.vocabulary.set(term, vocabIndex++);
        }
        docFreq.set(term, (docFreq.get(term) || 0) + 1);
      }
    }

    // Compute IDF for each term
    for (const [term, df] of docFreq) {
      // IDF = log(N / df) + 1 (smoothed to avoid log(0))
      const idf = Math.log((this.documentCount + 1) / (df + 1)) + 1;
      this.idf.set(term, idf);
    }
  }

  /**
   * Transform a document to a TF-IDF vector
   */
  transform(document: string): number[] {
    const terms = this.tokenize(document);
    const tf = this.computeTF(terms);
    const vector = new Array(this.vocabulary.size).fill(0);

    for (const [term, tfValue] of tf) {
      const idx = this.vocabulary.get(term);
      const idfValue = this.idf.get(term) || 1;
      if (idx !== undefined) {
        vector[idx] = tfValue * idfValue;
      }
    }

    return vector;
  }

  /**
   * Fit and transform in one step
   */
  fitTransform(documents: string[]): Map<string, number[]> {
    this.fit(documents);
    const vectors = new Map<string, number[]>();
    documents.forEach((doc, i) => {
      vectors.set(`doc_${i}`, this.transform(doc));
    });
    return vectors;
  }

  /**
   * Get vocabulary size
   */
  getVocabularySize(): number {
    return this.vocabulary.size;
  }

  /**
   * Serialize vocabulary for storage
   */
  serializeVocabulary(): { vocabulary: [string, number][]; idf: [string, number][] } {
    return {
      vocabulary: Array.from(this.vocabulary.entries()),
      idf: Array.from(this.idf.entries()),
    };
  }

  /**
   * Load vocabulary from storage
   */
  loadVocabulary(data: { vocabulary: [string, number][]; idf: [string, number][] }): void {
    this.vocabulary = new Map(data.vocabulary);
    this.idf = new Map(data.idf);
  }
}

// ─── Cosine Similarity ───────────────────────────────────────────────────────

/**
 * Compute cosine similarity between two vectors
 */
export function cosineSimilarity(a: number[], b: number[]): number {
  if (a.length !== b.length) return 0;

  let dotProduct = 0;
  let normA = 0;
  let normB = 0;

  for (let i = 0; i < a.length; i++) {
    const aVal = a[i] ?? 0;
    const bVal = b[i] ?? 0;
    dotProduct += aVal * bVal;
    normA += aVal * aVal;
    normB += bVal * bVal;
  }

  const denominator = Math.sqrt(normA) * Math.sqrt(normB);
  if (denominator === 0) return 0;

  return dotProduct / denominator;
}

// ─── Embeddings Storage ───────────────────────────────────────────────────────

/**
 * EmbeddingsStorage - SQLite-backed storage for entity embeddings
 */
export class EmbeddingsStorage {
  private db!: Database.Database;
  private vectorizer: TFIDFVectorizer;
  private initialized = false;

  constructor(private dbPath: string) {
    this.vectorizer = new TFIDFVectorizer();
  }

  /**
   * Initialize the database and load existing vocabulary
   */
  async init(): Promise<void> {
    const dir = path.dirname(this.dbPath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    this.db = new Database(this.dbPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('foreign_keys = ON');

    // Create tables
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS embeddings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        entity_id TEXT NOT NULL UNIQUE,
        text TEXT NOT NULL,
        vector TEXT NOT NULL,  -- JSON serialized array
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );
      
      CREATE TABLE IF NOT EXISTS vocabulary (
        id INTEGER PRIMARY KEY,
        term TEXT NOT NULL UNIQUE,
        idf REAL NOT NULL
      );
      
      CREATE INDEX IF NOT EXISTS idx_embeddings_entity ON embeddings(entity_id);
    `);

    // Load existing vocabulary
    this.loadVocabulary();

    this.initialized = true;
    console.log('[Embeddings] Initialized with vocabulary size:', this.vectorizer.getVocabularySize());
  }

  /**
   * Load vocabulary from database
   */
  private loadVocabulary(): void {
    try {
      const rows = this.db.prepare('SELECT term, idf FROM vocabulary ORDER BY id').all() as Array<{
        term: string;
        idf: number;
      }>;

      if (rows.length > 0) {
        const vocab = rows.map(r => [r.term, 0] as [string, number]);
        const idfVals = rows.map(r => [r.term, r.idf] as [string, number]);
        this.vectorizer.loadVocabulary({ vocabulary: vocab, idf: idfVals });
      }
    } catch (error) {
      console.warn('[Embeddings] No existing vocabulary found, starting fresh');
    }
  }

  /**
   * Save vocabulary to database
   */
  private saveVocabulary(): void {
    const serialized = this.vectorizer.serializeVocabulary();

    // Clear and repopulate
    const deleteStmt = this.db.prepare('DELETE FROM vocabulary');
    const insertStmt = this.db.prepare('INSERT INTO vocabulary (id, term, idf) VALUES (?, ?, ?)');

    const transaction = this.db.transaction(() => {
      deleteStmt.run();
      serialized.vocabulary.forEach(([term], idx) => {
        const idf = serialized.idf.find(([t]) => t === term)?.[1] || 1;
        insertStmt.run(idx, term, idf);
      });
    });

    transaction();
  }

  /**
   * Add or update embedding for an entity
   */
  async addEmbedding(entityId: string, text: string): Promise<void> {
    if (!this.initialized) await this.init();

    const now = Date.now();

    // Get all existing texts to recompute IDF
    const allRows = this.db.prepare('SELECT text FROM embeddings').all() as Array<{ text: string }>;
    const allTexts = allRows.map(r => r.text);

    // Add new text and recompute
    allTexts.push(text);
    this.vectorizer.fit(allTexts);

    // Transform to get vector
    const vector = this.vectorizer.transform(text);
    const vectorJson = JSON.stringify(vector);

    // Upsert
    const existing = this.db.prepare('SELECT id FROM embeddings WHERE entity_id = ?').get(entityId);

    if (existing) {
      this.db.prepare(`
        UPDATE embeddings SET text = ?, vector = ?, updated_at = ? WHERE entity_id = ?
      `).run(text, vectorJson, now, entityId);
    } else {
      this.db.prepare(`
        INSERT INTO embeddings (entity_id, text, vector, created_at, updated_at) 
        VALUES (?, ?, ?, ?, ?)
      `).run(entityId, text, vectorJson, now, now);
    }

    // Save updated vocabulary
    this.saveVocabulary();
  }

  /**
   * Get embedding for an entity
   */
  async getEmbedding(entityId: string): Promise<{ text: string; vector: number[] } | null> {
    if (!this.initialized) await this.init();

    const row = this.db.prepare('SELECT text, vector FROM embeddings WHERE entity_id = ?').get(entityId) as {
      text: string;
      vector: string;
    } | undefined;

    if (!row) return null;

    return {
      text: row.text,
      vector: JSON.parse(row.vector),
    };
  }

  /**
   * Delete embedding for an entity
   */
  async deleteEmbedding(entityId: string): Promise<boolean> {
    if (!this.initialized) await this.init();

    const result = this.db.prepare('DELETE FROM embeddings WHERE entity_id = ?').run(entityId);
    return result.changes > 0;
  }

  /**
   * Find similar entities using cosine similarity
   */
  async findSimilar(text: string, k = 5, options: SearchOptions = {}): Promise<SimilarityResult[]> {
    if (!this.initialized) await this.init();

    const limit = options.limit || k;
    const minSimilarity = options.minSimilarity || 0;

    // Get query vector
    const queryVector = this.vectorizer.transform(text);

    // Get all embeddings
    const rows = this.db.prepare('SELECT entity_id, text, vector FROM embeddings').all() as Array<{
      entity_id: string;
      text: string;
      vector: string;
    }>;

    // Compute similarities
    const results: SimilarityResult[] = [];

    for (const row of rows) {
      const vector = JSON.parse(row.vector);
      const similarity = cosineSimilarity(queryVector, vector);

      if (similarity >= minSimilarity) {
        results.push({
          entity_id: row.entity_id,
          text: row.text,
          similarity,
        });
      }
    }

    // Sort by similarity descending
    results.sort((a, b) => b.similarity - a.similarity);

    return results.slice(0, limit);
  }

  /**
   * Keyword-based fallback search when vectors unavailable
   */
  async keywordSearch(query: string, k = 5): Promise<SimilarityResult[]> {
    if (!this.initialized) await this.init();

    const terms = query.toLowerCase().split(/\s+/).filter(t => t.length > 1);

    if (terms.length === 0) return [];

    // Build SQL LIKE clauses
    const likeClauses = terms.map(() => 'text LIKE ?');
    const params = terms.map(t => `%${t}%`);

    const rows = this.db.prepare(`
      SELECT entity_id, text FROM embeddings WHERE ${likeClauses.join(' OR ')}
    `).all(...params) as Array<{ entity_id: string; text: string }>;

    // Score by number of matching terms
    const results: SimilarityResult[] = rows.map(row => {
      const textLower = row.text.toLowerCase();
      const matchCount = terms.filter(t => textLower.includes(t)).length;
      return {
        entity_id: row.entity_id,
        text: row.text,
        similarity: matchCount / terms.length,  // Normalized by query terms
      };
    });

    results.sort((a, b) => b.similarity - a.similarity);
    return results.slice(0, k);
  }

  /**
   * Rebuild all embeddings from a list of texts
   * Useful when adding many entities at once
   */
  async rebuildIndex(entities: Array<{ id: string; text: string }>): Promise<void> {
    if (!this.initialized) await this.init();

    const texts = entities.map(e => e.text);
    const now = Date.now();

    // Fit vectorizer on all texts
    this.vectorizer.fit(texts);

    // Clear existing
    this.db.prepare('DELETE FROM embeddings').run();

    // Insert all with new vectors
    const insertStmt = this.db.prepare(`
      INSERT INTO embeddings (entity_id, text, vector, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?)
    `);

    const transaction = this.db.transaction(() => {
      for (const entity of entities) {
        const vector = this.vectorizer.transform(entity.text);
        insertStmt.run(entity.id, entity.text, JSON.stringify(vector), now, now);
      }
    });

    transaction();

    // Save vocabulary
    this.saveVocabulary();

    console.log('[Embeddings] Rebuilt index with', entities.length, 'entities');
  }

  /**
   * Get statistics
   */
  async getStats(): Promise<{
    count: number;
    vocabularySize: number;
    avgVectorLength: number;
  }> {
    if (!this.initialized) await this.init();

    const countResult = this.db.prepare('SELECT COUNT(*) as c FROM embeddings').get() as { c: number };
    const firstRow = this.db.prepare('SELECT vector FROM embeddings LIMIT 1').get() as { vector: string } | undefined;

    return {
      count: countResult.c,
      vocabularySize: this.vectorizer.getVocabularySize(),
      avgVectorLength: firstRow ? JSON.parse(firstRow.vector).length : 0,
    };
  }

  /**
   * Close database connection
   */
  close(): void {
    if (this.db) {
      this.db.close();
    }
  }
}

// ─── Singleton ────────────────────────────────────────────────────────────────

let embeddingsInstance: EmbeddingsStorage | null = null;

export function getEmbeddingsStorage(dbPath?: string): EmbeddingsStorage {
  if (!embeddingsInstance) {
    const defaultPath = path.join(process.cwd(), 'data', 'embeddings.db');
    embeddingsInstance = new EmbeddingsStorage(dbPath || defaultPath);
  }
  return embeddingsInstance;
}

export async function initEmbeddings(dbPath?: string): Promise<EmbeddingsStorage> {
  const storage = getEmbeddingsStorage(dbPath);
  await storage.init();
  return storage;
}
