/**
 * Error Analyzer
 *
 * Phase 1 of the self-improving loop.
 * Reviews failed tasks, user corrections, and identifies patterns.
 */

import * as fs from 'fs';
import { existsSync, mkdirSync } from 'fs';
import * as path from 'path';

import type { ErrorAnalysis, UserCorrection } from './types';

export class ErrorAnalyzer {
  private sessionsDbPath: string;
  private memoryDir: string;
  private lessonsPath: string;

  constructor(sessionsDbPath: string, memoryDir: string) {
    this.sessionsDbPath = sessionsDbPath;
    this.memoryDir = memoryDir;
    this.lessonsPath = path.join(memoryDir, 'self-improvement', 'lessons.md');
  }

  /**
   * Analyze errors from session history and lessons log
   */
  async analyzeErrors(): Promise<ErrorAnalysis[]> {
    const analyses: ErrorAnalysis[] = [];
    const _corrections = await this.findUserCorrections();
    const failedTasks = await this.findFailedTasks();
    const _existingLessons = this.loadExistingLessons();

    // Group corrections by pattern
    const patternGroups = new Map<string, UserCorrection[]>();
    for (const corr of corrections) {
      const pattern = this.categorizeCorrection(corr);
      if (!patternGroups.has(pattern)) {
        patternGroups.set(pattern, []);
      }
      patternGroups.get(pattern)!.push(corr);
    }

    // Build analyses from patterns
    for (const [pattern, corrs] of patternGroups.entries()) {
      if (corrs.length < 1) continue;

      const analysis: ErrorAnalysis = {
        id: `err:${Date.now()}:${Math.random().toString(36).slice(2, 6)}`,
        timestamp: Date.now(),
        sessionId: corrs[0]?.sessionId ?? 'unknown',
        errorType: 'correction',
        description: this.describePattern(pattern, corrs),
        rootCause: this.inferRootCause(pattern, corrs),
        pattern,
        frequency: corrs.length,
        lessons: this.extractLessons(pattern, corrs),
        fixSuggestion: this.suggestFix(pattern, corrs),
      };

      analyses.push(analysis);
    }

    // Add failed task analyses
    for (const task of failedTasks) {
      analyses.push(task);
    }

    return analyses;
  }

  /**
   * Find user corrections from recent sessions
   */
  private async findUserCorrections(): Promise<UserCorrection[]> {
    const _corrections: UserCorrection[] = [];

    if (!existsSync(this.sessionsDbPath)) {
      return corrections;
    }

    try {
      // Dynamic require to avoid issues when better-sqlite3 isn't available
      const Database = require('better-sqlite3');
      const db = new Database(this.sessionsDbPath, { readonly: true });

      // Get recent messages where user may have corrected the assistant
      const cutoff = Date.now() - 7 * 24 * 60 * 60 * 1000; // last 7 days
      const messages = db
        .prepare(
          `SELECT m.*, s.title as session_title
         FROM messages m
         JOIN sessions s ON m.session_id = s.id
         WHERE m.timestamp >= ?
         AND m.role = 'user'
         ORDER BY m.timestamp DESC
         LIMIT 500`,
        )
        .all(cutoff) as any[];

      for (let i = 0; i < messages.length - 1; i++) {
        const msg = messages[i]!;
        const nextMsg = messages[i + 1];

        // Look for correction patterns in user messages
        const content = msg.content ?? '';
        const correctionIndicators = [
          'actually', 'no,', 'not quite', 'wrong', 'incorrect',
          'should be', 'i meant', 'I meant', 'better:', 'correct:',
          'you misunderstood', 'that\'s not what i asked',
        ];

        const isCorrection = correctionIndicators.some(ind => content.toLowerCase().includes(ind));

        if (isCorrection && nextMsg?.role === 'assistant') {
          corrections.push({
            sessionId: msg.session_id,
            timestamp: msg.timestamp,
            originalResponse: nextMsg.content,
            correctedResponse: content,
            context: this.extractContext(messages, i),
            category: this.categorizeCorrectionText(content),
          });
        }
      }

      db.close();
    } catch (err) {
      // Silently handle - sessions DB might not exist
    }

    return corrections;
  }

  /**
   * Find failed tasks from existing lessons
   */
  private async findFailedTasks(): Promise<ErrorAnalysis[]> {
    const analyses: ErrorAnalysis[] = [];

    if (!existsSync(this.lessonsPath)) {
      return analyses;
    }

    try {
      const content = fs.readFileSync(this.lessonsPath, 'utf8');
      const failedMatches = content.matchAll(
        /## FAILED[^\n]*\n([^\n]*\n){0,3}?([^\n]*error[^\n]*|failed|wrong|incorrect)/gi
      );

      for (const match of failedMatches) {
        const block = match[0];
        if (block.includes('error') || block.includes('failed')) {
          analyses.push({
            id: `failed:${Date.now()}:${Math.random().toString(36).slice(2, 6)}`,
            timestamp: Date.now(),
            sessionId: 'lessons-log',
            errorType: 'failure',
            description: block.slice(0, 200),
            rootCause: 'Detected from previous lesson logs',
            pattern: 'historical_failure',
            frequency: 1,
            lessons: ['Review previous failure for patterns'],
            fixSuggestion: 'Check lessons.md for context',
          });
        }
      }
    } catch {
      // Ignore read errors
    }

    return analyses;
  }

  /**
   * Load existing lessons from log
   */
  private loadExistingLessons(): string[] {
    if (!existsSync(this.lessonsPath)) {
      return [];
    }
    try {
      return fs.readFileSync(this.lessonsPath, 'utf8').split('\n')
        .filter(l => l.includes('- lesson:') || l.includes('lesson:'));
    } catch {
      return [];
    }
  }

  /**
   * Categorize a correction into a pattern
   */
  private categorizeCorrection(corr: UserCorrection): string {
    const text = corr.correctedResponse.toLowerCase();

    if (text.includes('format') || text.includes('output') || text.includes('structure')) {
      return 'output_formatting';
    }
    if (text.includes('wrong') || text.includes('incorrect') || text.includes('not right')) {
      return 'wrong_answer';
    }
    if (text.includes('too long') || text.includes('too brief') || text.includes('shorten')) {
      return 'response_length';
    }
    if (text.includes('clarify') || text.includes('unclear') || text.includes('confusing')) {
      return 'clarification_needed';
    }
    if (text.includes('code') || text.includes('programming') || text.includes('syntax')) {
      return 'code_quality';
    }
    if (text.includes('miss') || text.includes('forgot') || text.includes('didn\'t include')) {
      return 'incomplete_response';
    }

    return 'general_correction';
  }

  /**
   * Categorize correction text
   */
  private categorizeCorrectionText(text: string): string {
    return this.categorizeCorrection({
      sessionId: '', timestamp: 0, originalResponse: '', correctedResponse: text, context: '', category: ''
    });
  }

  /**
   * Extract context around a message
   */
  private extractContext(messages: any[], index: number): string {
    const before = messages.slice(Math.max(0, index - 2), index);
    const after = messages.slice(index + 1, Math.min(messages.length, index + 3));

    const parts: string[] = [];
    for (const m of before) {
      parts.push(`[${m.role}]: ${(m.content ?? '').slice(0, 100)}`);
    }
    for (const m of after) {
      parts.push(`[${m.role}]: ${(m.content ?? '').slice(0, 100)}`);
    }

    return parts.join(' | ').slice(0, 300);
  }

  /**
   * Describe a pattern in human-readable form
   */
  private describePattern(pattern: string, _corrections: UserCorrection[]): string {
    const count = corrections.length;
    switch (pattern) {
      case 'output_formatting':
        return `User corrected output formatting ${count} time${count > 1 ? 's' : ''}`;
      case 'wrong_answer':
        return `User flagged wrong answers ${count} time${count > 1 ? 's' : ''}`;
      case 'response_length':
        return `User commented on response length ${count} time${count > 1 ? 's' : ''}`;
      case 'clarification_needed':
        return `User requested clarification ${count} time${count > 1 ? 's' : ''}`;
      case 'code_quality':
        return `User raised code quality concerns ${count} time${count > 1 ? 's' : ''}`;
      case 'incomplete_response':
        return `User noted incomplete responses ${count} time${count > 1 ? 's' : ''}`;
      default:
        return `General corrections: ${count} occurrence${count > 1 ? 's' : ''}`;
    }
  }

  /**
   * Infer root cause from pattern
   */
  private inferRootCause(pattern: string, _corrections: UserCorrection[]): string {
    const causes: Record<string, string> = {
      output_formatting: 'LLM may not follow output structure instructions precisely',
      wrong_answer: 'Model hallucinated or lacked sufficient context',
      response_length: 'Model miscalibrated response verbosity',
      clarification_needed: 'Ambiguous user intent not properly resolved before responding',
      code_quality: 'Generated code may have style issues or missing edge cases',
      incomplete_response: 'Model may be cutting off or not fully addressing the query',
    };
    return causes[pattern] ?? 'Unknown root cause - requires manual review';
  }

  /**
   * Extract lessons from pattern
   */
  private extractLessons(pattern: string, _corrections: UserCorrection[]): string[] {
    const lessons: string[] = [];
    switch (pattern) {
      case 'output_formatting':
        lessons.push('Include explicit format instructions in system prompt');
        lessons.push('Validate output structure before responding');
        break;
      case 'wrong_answer':
        lessons.push('Verify facts against known information before stating');
        lessons.push('Ask clarifying questions if uncertain');
        break;
      case 'response_length':
        lessons.push('Check user history for preferred response length');
        lessons.push('Use user-modeling data to calibrate verbosity');
        break;
      case 'clarification_needed':
        lessons.push('Ask one clarifying question before extensive response');
        lessons.push('Paraphrase user request back to confirm understanding');
        break;
      case 'code_quality':
        lessons.push('Include code style preferences in skill context');
        lessons.push('Add validation for generated code');
        break;
      case 'incomplete_response':
        lessons.push('Break complex tasks into numbered steps');
        lessons.push('Verify all parts of user request are addressed');
        break;
    }
    return lessons;
  }

  /**
   * Suggest a fix for pattern
   */
  private suggestFix(pattern: string, _corrections: UserCorrection[]): string {
    const fixes: Record<string, string> = {
      output_formatting: 'Add output_formatting to response checklist in SOUL.md',
      wrong_answer: 'Add verification step before claiming factual information',
      response_length: 'Update user-modeling.md with preferred response length',
      clarification_needed: 'Add clarification protocol to AGENTS.md',
      code_quality: 'Create code_style skill with preferred conventions',
      incomplete_response: 'Add task completion checklist before sending response',
    };
    return fixes[pattern] ?? 'Review manually and update relevant skill or config';
  }

  /**
   * Log lessons learned to lessons.md
   */
  logLessons(analyses: ErrorAnalysis[]): void {
    const dir = path.join(this.memoryDir, 'self-improvement');
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }

    const timestamp = new Date().toISOString();
    let content = `\n## ${timestamp}\n\n`;

    for (const a of analyses) {
      content += `### ${a.pattern} (x${a.frequency})\n`;
      content += `- Root cause: ${a.rootCause}\n`;
      content += `- Fix: ${a.fixSuggestion}\n`;
      for (const lesson of a.lessons) {
        content += `- lesson: ${lesson}\n`;
      }
      content += '\n';
    }

    try {
      fs.appendFileSync(this.lessonsPath, content, 'utf8');
    } catch {
      // Ignore write errors
    }
  }
}
