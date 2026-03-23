/**
 * Document Analysis Feature
 *
 * Document processing and analysis for PDF, DOCX, TXT, and Markdown files.
 * Provides text extraction, metadata retrieval, and summarization structure.
 * Real PDF/DOCX parsing would require additional libraries (pdf-parse, mammoth).
 */

import { mkdirSync, existsSync, statSync, readFileSync } from 'fs';
import { dirname, extname, basename, join } from 'path';

import Database from 'better-sqlite3';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** Supported document types */
export type DocumentType = 'pdf' | 'docx' | 'txt' | 'md';

/** Document metadata */
export interface DocumentMetadata {
  path: string;
  name: string;
  type: DocumentType;
  size: number; // bytes
  pages?: number; // For PDF/DOCX
  author?: string;
  title?: string;
  createdAt: number;
  modifiedAt: number;
  wordCount?: number;
  language?: string;
}

/** Text extraction result */
export interface TextExtraction {
  metadata: DocumentMetadata;
  text: string;
  sections?: Array<{ title: string; content: string }>;
  tables?: unknown[]; // Future: extracted tables
}

/** Analysis result (full document analysis) */
export interface AnalysisResult {
  metadata: DocumentMetadata;
  text: string;
  summary?: string;
  wordCount: number;
  charCount: number;
  readingTimeMinutes?: number;
  topWords?: Array<{ word: string; count: number }>;
  language?: string;
  sentiment?: 'positive' | 'neutral' | 'negative';
  processedAt: number;
}

/** Feature configuration */
export interface DocumentAnalysisConfig {
  cacheDir?: string;
  maxCacheEntries?: number;
  summaryProvider?: 'local' | 'llm'; // Future: use LLM for summarization
}

/**
 * DocumentAnalysis — document processing and insights.
 *
 * Provides:
 * - Text extraction from PDF, DOCX, TXT, Markdown
 * - Document metadata retrieval
 * - Basic text analysis (word count, reading time)
 * - Summarization structure (stub for LLM integration)
 * - Analysis caching in SQLite
 *
 * For full PDF/DOCX support, add optional dependencies: pdf-parse, mammoth.
 */
class DocumentAnalysisFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'document-analysis',
    version: '0.1.0',
    description: 'Document analysis for PDF, DOCX, TXT, MD with text extraction and insights',
    dependencies: [],
  };

  private config: Required<DocumentAnalysisConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;

  constructor() {
    this.config = {
      cacheDir: './data/document-analysis',
      maxCacheEntries: 1000,
      summaryProvider: 'local',
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      cacheDir: (config['cacheDir'] as string) ?? this.config['cacheDir'],
      maxCacheEntries: (config['maxCacheEntries'] as number) ?? this.config['maxCacheEntries'],
      summaryProvider:
        (config['summaryProvider'] as 'local' | 'llm') ?? this.config['summaryProvider'],
    };

    this.initDatabase();

    if (!existsSync(this.config.cacheDir)) {
      mkdirSync(this.config.cacheDir, { recursive: true });
    }
  }

  async start(): Promise<void> {
    this.ctx.logger.info('DocumentAnalysis active', {
      cacheDir: this.config.cacheDir,
    });
  }

  async stop(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.ctx.logger.info('DocumentAnalysis stopped');
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const cached = (
        this.db.prepare('SELECT COUNT(*) as c FROM analysis_cache').get() as { c: number }
      ).c;

      return {
        healthy: true,
        details: {
          cachedAnalyses: cached,
        },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /**
   * Extract text from a document.
   *
   * @param filePath - Path to document file
   * @returns TextExtraction result with metadata and content
   */
  async extractText(filePath: string): Promise<TextExtraction> {
    const metadata = this.getMetadata(filePath);
    this.ctx.logger.debug('Extracting text', { file: filePath, type: metadata.type });

    let text: string;

    try {
      switch (metadata.type) {
        case 'txt':
          text = readFileSync(filePath, 'utf-8');
          break;
        case 'md':
          text = readFileSync(filePath, 'utf-8');
          // Could strip markdown syntax for pure text, but keeping as-is
          break;
        case 'pdf':
          text = await this.extractPdfText(filePath);
          break;
        case 'docx':
          text = await this.extractDocxText(filePath);
          break;
        default:
          throw new Error(`Unsupported document type: ${metadata.type}`);
      }

      // Basic section detection for markdown
      let sections: TextExtraction['sections'] = undefined;
      if (metadata.type === 'md') {
        sections = this.splitMarkdownSections(text);
      }

      return {
        metadata,
        text,
        sections,
      };
    } catch (err) {
      this.ctx.logger.error('Text extraction failed', {
        file: filePath,
        error: err instanceof Error ? err.message : String(err),
      });
      throw err;
    }
  }

  /**
   * Get document metadata.
   */
  getMetadata(filePath: string): DocumentMetadata {
    if (!existsSync(filePath)) {
      throw new Error(`File not found: ${filePath}`);
    }

    const stats = statSync(filePath);
    const ext = extname(filePath).slice(1).toLowerCase() as DocumentType;
    const supported: DocumentType[] = ['pdf', 'docx', 'txt', 'md'];

    if (!supported.includes(ext)) {
      throw new Error(`Unsupported file type: ${ext}`);
    }

    const metadata: DocumentMetadata = {
      path: filePath,
      name: basename(filePath),
      type: ext,
      size: stats.size,
      createdAt: stats.ctimeMs,
      modifiedAt: stats.mtimeMs,
    };

    // Extract additional metadata based on type
    if (ext === 'pdf') {
      // Could use pdf-parse library if available
      metadata.pages = 0; // placeholder
      metadata.title = undefined;
      metadata.author = undefined;
    } else if (ext === 'docx') {
      // Could use mammoth library if available
      metadata.pages = 0;
      metadata.author = undefined;
      metadata.title = undefined;
    } else if (ext === 'txt' || ext === 'md') {
      // Count words
      const content = readFileSync(filePath, 'utf-8');
      metadata.wordCount = content.split(/\s+/).filter(Boolean).length;
    }

    return metadata;
  }

  /**
   * Perform full document analysis.
   *
   * @param filePath - Path to document
   * @param force - Re-analyze even if cached
   * @returns AnalysisResult with text, summary, and insights
   */
  async analyze(filePath: string, force: boolean = false): Promise<AnalysisResult> {
    const stats = statSync(filePath);
    const cacheKey = `${filePath}:${stats.mtimeMs}:${stats.size}`;

    // Check cache
    if (!force) {
      const cached = this.db
        .prepare('SELECT * FROM analysis_cache WHERE cache_key = ?')
        .get(cacheKey) as { result: string } | undefined;
      if (cached) {
        this.ctx.logger.debug('Analysis cache hit', { file: filePath });
        return JSON.parse(cached.result);
      }
    }

    this.ctx.logger.info('Analyzing document', { file: filePath });

    // Extract text
    const extraction = await this.extractText(filePath);
    const text = extraction.text.trim();

    const wordCount = text.split(/\s+/).filter(Boolean).length;
    const charCount = text.length;
    const readingTime = Math.ceil(wordCount / 200); // 200 wpm

    // Basic summary (stub - could use LLM)
    const summary = this.generateSummary(text);

    // Detect language (very basic)
    const language = this.detectLanguage(text);

    // Top words (simple frequency)
    const topWords = this.getTopWords(text, 10);

    // Sentiment (very basic keyword matching)
    const sentiment = this.estimateSentiment(text);

    const result: AnalysisResult = {
      metadata: extraction.metadata,
      text,
      summary,
      wordCount,
      charCount,
      readingTimeMinutes: readingTime,
      topWords,
      language,
      sentiment,
      processedAt: Date.now(),
    };

    // Cache result
    this.db
      .prepare(
        `
      INSERT OR REPLACE INTO analysis_cache (cache_key, file_path, result, cached_at)
      VALUES (?, ?, ?, ?)
    `,
      )
      .run(cacheKey, filePath, JSON.stringify(result), Date.now());

    this.ctx.logger.info('Analysis complete', {
      file: filePath,
      words: wordCount,
      language,
      sentiment,
    });

    return result;
  }

  /**
   * Summarize a document (stub).
   *
   * @param filePath - Document file
   * @returns Summary text
   */
  async summarize(filePath: string): Promise<string> {
    const analysis = await this.analyze(filePath);
    return analysis.summary ?? '';
  }

  /**
   * Clear analysis cache for a specific file.
   */
  clearCacheForFile(filePath: string): boolean {
    const result = this.db.prepare('DELETE FROM analysis_cache WHERE file_path = ?').run(filePath);
    return result.changes > 0;
  }

  /**
   * Clear old cache entries.
   */
  clearCache(olderThanDays: number = 7): number {
    const cutoff = Date.now() - olderThanDays * 24 * 60 * 60 * 1000;
    const result = this.db.prepare('DELETE FROM analysis_cache WHERE cached_at < ?').run(cutoff);
    return result.changes;
  }

  /** Extract text from PDF (stub - would use pdf-parse) */
  private async extractPdfText(filePath: string): Promise<string> {
    // In real implementation, would use pdf-parse or pdfjs-dist
    // This stub returns placeholder
    return `[PDF EXTRACTION STUB]\nFile: ${filePath}\nUse pdf-parse library for full extraction.\n`;
  }

  /** Extract text from DOCX (stub - would use mammoth) */
  private async extractDocxText(filePath: string): Promise<string> {
    // In real implementation, would use mammoth or similar
    return `[DOCX EXTRACTION STUB]\nFile: ${filePath}\nUse mammoth library for full extraction.\n`;
  }

  /** Split markdown into sections by headings */
  private splitMarkdownSections(text: string): Array<{ title: string; content: string }> {
    const sections: Array<{ title: string; content: string }> = [];
    const lines = text.split('\n');
    let currentTitle = 'Introduction';
    let currentContent: string[] = [];

    for (const line of lines) {
      if (line.startsWith('# ') || line.startsWith('## ') || line.startsWith('### ')) {
        if (currentContent.length > 0) {
          sections.push({ title: currentTitle, content: currentContent.join('\n').trim() });
          currentContent = [];
        }
        currentTitle = line.replace(/^#+\s/, '');
      } else {
        currentContent.push(line);
      }
    }

    if (currentContent.length > 0) {
      sections.push({ title: currentTitle, content: currentContent.join('\n').trim() });
    }

    return sections;
  }

  /** Generate a basic summary (first N sentences) */
  private generateSummary(text: string, maxSentences: number = 3): string {
    const sentences = text.split(/[.!?]+/).filter((s) => s.trim().length > 0);
    if (sentences.length === 0) return '';
    return `${sentences.slice(0, maxSentences).join('. ')}.`;
  }

  /** Very basic language detection */
  private detectLanguage(text: string): string {
    // Check for common Cyrillic characters
    if (/[а-яА-ЯёЁ]/.test(text)) return 'ru';
    // Check for common Estonian characters
    if (/[äöõüšž]/i.test(text)) return 'et';
    // Default to English if mostly ASCII letters
    if (/[a-zA-Z]/.test(text)) return 'en';
    return 'unknown';
  }

  /** Get top N most frequent words */
  private getTopWords(text: string, n: number): Array<{ word: string; count: number }> {
    const words = text.toLowerCase().match(/\b[a-z]{3,}\b/g) ?? [];
    const freq = new Map<string, number>();
    for (const w of words) {
      freq.set(w, (freq.get(w) ?? 0) + 1);
    }
    return Array.from(freq.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, n)
      .map(([word, count]) => ({ word, count }));
  }

  /** Estimate sentiment based on simple keyword matching */
  private estimateSentiment(text: string): 'positive' | 'neutral' | 'negative' {
    const lower = text.toLowerCase();
    const positive = [
      'good',
      'great',
      'excellent',
      'love',
      'happy',
      'wonderful',
      'amazing',
      'positive',
    ].filter((w) => lower.includes(w)).length;
    const negative = [
      'bad',
      'terrible',
      'awful',
      'hate',
      'sad',
      'horrible',
      'negative',
      'worst',
    ].filter((w) => lower.includes(w)).length;

    if (positive > negative) return 'positive';
    if (negative > positive) return 'negative';
    return 'neutral';
  }

  /** Initialize database */
  private initDatabase(): void {
    const dbPath = join(this.config.cacheDir, 'cache.db');
    if (!existsSync(dirname(dbPath))) {
      mkdirSync(dirname(dbPath), { recursive: true });
    }

    this.db = new Database(dbPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS analysis_cache (
        cache_key TEXT PRIMARY KEY,
        file_path TEXT NOT NULL,
        result TEXT NOT NULL,
        cached_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_cache_file ON analysis_cache(file_path);
      CREATE INDEX IF NOT EXISTS idx_cache_cached_at ON analysis_cache(cached_at DESC);
    `);
  }
}

export default new DocumentAnalysisFeature();
