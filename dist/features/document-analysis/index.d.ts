/**
 * Document Analysis Feature
 *
 * Document processing and analysis for PDF, DOCX, TXT, and Markdown files.
 * Provides text extraction, metadata retrieval, and summarization structure.
 * Real PDF/DOCX parsing would require additional libraries (pdf-parse, mammoth).
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Supported document types */
export type DocumentType = 'pdf' | 'docx' | 'txt' | 'md';
/** Document metadata */
export interface DocumentMetadata {
    path: string;
    name: string;
    type: DocumentType;
    size: number;
    pages?: number;
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
    sections?: Array<{
        title: string;
        content: string;
    }>;
    tables?: unknown[];
}
/** Analysis result (full document analysis) */
export interface AnalysisResult {
    metadata: DocumentMetadata;
    text: string;
    summary?: string;
    wordCount: number;
    charCount: number;
    readingTimeMinutes?: number;
    topWords?: Array<{
        word: string;
        count: number;
    }>;
    language?: string;
    sentiment?: 'positive' | 'neutral' | 'negative';
    processedAt: number;
}
/** Feature configuration */
export interface DocumentAnalysisConfig {
    cacheDir?: string;
    maxCacheEntries?: number;
    summaryProvider?: 'local' | 'llm';
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
declare class DocumentAnalysisFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Extract text from a document.
     *
     * @param filePath - Path to document file
     * @returns TextExtraction result with metadata and content
     */
    extractText(filePath: string): Promise<TextExtraction>;
    /**
     * Get document metadata.
     */
    getMetadata(filePath: string): DocumentMetadata;
    /**
     * Perform full document analysis.
     *
     * @param filePath - Path to document
     * @param force - Re-analyze even if cached
     * @returns AnalysisResult with text, summary, and insights
     */
    analyze(filePath: string, force?: boolean): Promise<AnalysisResult>;
    /**
     * Summarize a document (stub).
     *
     * @param filePath - Document file
     * @returns Summary text
     */
    summarize(filePath: string): Promise<string>;
    /**
     * Clear analysis cache for a specific file.
     */
    clearCacheForFile(filePath: string): boolean;
    /**
     * Clear old cache entries.
     */
    clearCache(olderThanDays?: number): number;
    /** Extract text from PDF (stub - would use pdf-parse) */
    private extractPdfText;
    /** Extract text from DOCX (stub - would use mammoth) */
    private extractDocxText;
    /** Split markdown into sections by headings */
    private splitMarkdownSections;
    /** Generate a basic summary (first N sentences) */
    private generateSummary;
    /** Very basic language detection */
    private detectLanguage;
    /** Get top N most frequent words */
    private getTopWords;
    /** Estimate sentiment based on simple keyword matching */
    private estimateSentiment;
    /** Initialize database */
    private initDatabase;
}
declare const _default: DocumentAnalysisFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map