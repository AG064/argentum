/**
 * Auto-Capture Feature
 *
 * OMEGA Memory integration — automatically detects and captures
 * decisions, lessons, errors, and preferences from conversations.
 */

import { FeatureModule, FeatureContext, FeatureMeta, HealthStatus } from '../../core/plugin-loader';
import { getSemanticMemory } from '../../memory/semantic';
import { getMemoryGraph } from '../../memory/graph';

/** Capture configuration */
export interface AutoCaptureConfig {
  enabled: boolean;
  captureDecisions: boolean;
  captureLessons: boolean;
  captureErrors: boolean;
  capturePreferences: boolean;
  minConfidence: number;
}

/** Detected capture item */
export interface CaptureItem {
  type: 'decision' | 'lesson' | 'error' | 'preference' | 'general';
  content: string;
  confidence: number;
  source: string;
}

/** Pattern definition for detection */
interface CapturePattern {
  type: CaptureItem['type'];
  patterns: RegExp[];
  keywords: string[];
  confidence: number;
}

/**
 * AutoCapture — detects valuable information in conversation text.
 *
 * Patterns for detection:
 * - Decisions: "let's use", "I'll go with", "we decided", "the plan is"
 * - Lessons: "I learned", "don't do", "the trick is", "pro tip"
 * - Errors: "the error was", "fixed by", "the issue was", "bug was"
 * - Preferences: "I prefer", "always use", "never use", "best practice"
 */
class AutoCaptureFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'auto-capture',
    version: '0.1.0',
    description: 'Automatic capture of decisions, lessons, errors, and preferences',
    dependencies: [],
  };

  private config: AutoCaptureConfig = {
    enabled: false,
    captureDecisions: true,
    captureLessons: true,
    captureErrors: true,
    capturePreferences: true,
    minConfidence: 0.5,
  };
  private ctx!: FeatureContext;
  private capturedCount = 0;
  private patterns: CapturePattern[] = [];

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<AutoCaptureConfig>) };
    this.initPatterns();

    // Register hook for message processing
    context.registerHook('message:received', this.handleMessage.bind(this));
    context.registerHook('message:sent', this.handleResponse.bind(this));
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Auto-Capture active', {
      decisions: this.config.captureDecisions,
      lessons: this.config.captureLessons,
      errors: this.config.captureErrors,
      preferences: this.config.capturePreferences,
    });
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: true,
      details: {
        captured: this.capturedCount,
        patterns: this.patterns.length,
      },
    };
  }

  /** Initialize detection patterns */
  private initPatterns(): void {
    if (this.config.captureDecisions) {
      this.patterns.push({
        type: 'decision',
        patterns: [
          /(?:let'?s|we'?ll|I'?ll)\s+(?:use|go\s+with|pick|choose|adopt|implement)\s+(.+)/gi,
          /(?:decided|agreed|settled)\s+(?:to|on|that)\s+(.+)/gi,
          /(?:the\s+)?(?:plan|approach|strategy|decision)\s+is\s+(?:to\s+)?(.+)/gi,
          /(?:going\s+with|chose|selected)\s+(.+)/gi,
          /(?:I\s+think\s+)?(?:we\s+should|let'?s\s+just)\s+(.+)/gi,
        ],
        keywords: ["let's use", "go with", "decided", "plan is", "we'll use", "going with", "we should"],
        confidence: 0.7,
      });
    }

    if (this.config.captureLessons) {
      this.patterns.push({
        type: 'lesson',
        patterns: [
          /(?:I|we)\s+learn(?:ed|t)\s+(?:that\s+)?(.+)/gi,
          /(?:don'?t|never|avoid)\s+(?:do|use|try)\s+(.+?)(?:\s+because\s+(.+))?/gi,
          /(?:the\s+)?(?:trick|tip|secret|key)\s+is\s+(?:to\s+)?(.+)/gi,
          /(?:pro\s+tip|lesson\s+learned|good\s+to\s+know)[:\s]+(.+)/gi,
          /(?:in\s+the\s+future|next\s+time|from\s+now\s+on)[,:\s]+(.+)/gi,
        ],
        keywords: ["I learned", "don't do", "trick is", "pro tip", "lesson", "good to know", "next time"],
        confidence: 0.8,
      });
    }

    if (this.config.captureErrors) {
      this.patterns.push({
        type: 'error',
        patterns: [
          /(?:the\s+)?(?:error|issue|problem|bug|crash)\s+(?:was|is)\s+(.+)/gi,
          /(?:fixed|resolved|solved)\s+(?:by|it\s+by)\s+(.+)/gi,
          /(?:root\s+cause|turned\s+out)\s+(?:was\s+)?(?:to\s+be\s+)?(.+)/gi,
          /(?:the\s+)?(?:fix|workaround|solution)\s+(?:was|is)\s+(?:to\s+)?(.+)/gi,
          /(?:broke|failed|crashed)\s+(?:because|when|due\s+to)\s+(.+)/gi,
        ],
        keywords: ["error was", "fixed by", "issue was", "bug was", "root cause", "the fix is", "broke because"],
        confidence: 0.8,
      });
    }

    if (this.config.capturePreferences) {
      this.patterns.push({
        type: 'preference',
        patterns: [
          /(?:I|we)\s+prefer(?:s|red)?\s+(.+)/gi,
          /(?:always|usually|typically)\s+use\s+(.+)/gi,
          /(?:never|don'?t)\s+use\s+(.+)/gi,
          /(?:best\s+practice|recommended)\s+(?:is\s+)?(?:to\s+)?(.+)/gi,
          /(?:I|we)\s+(?:like|love|hate|avoid)\s+(?:using\s+)?(.+)/gi,
        ],
        keywords: ["I prefer", "always use", "never use", "best practice", "I like", "I avoid"],
        confidence: 0.6,
      });
    }
  }

  /** Handle incoming message */
  private async handleMessage(data: unknown): Promise<void> {
    const text = this.extractText(data);
    if (!text) return;

    const captures = this.detectCaptures(text, 'user');
    for (const capture of captures) {
      await this.saveCapture(capture);
    }
  }

  /** Handle outgoing response */
  private async handleResponse(data: unknown): Promise<void> {
    const text = this.extractText(data);
    if (!text) return;

    const captures = this.detectCaptures(text, 'assistant');
    for (const capture of captures) {
      await this.saveCapture(capture);
    }
  }

  /** Extract text from hook data */
  private extractText(data: unknown): string | null {
    if (typeof data === 'string') return data;
    if (typeof data === 'object' && data !== null) {
      const obj = data as Record<string, unknown>;
      return (obj['text'] ?? obj['content'] ?? obj['message'] ?? null) as string | null;
    }
    return null;
  }

  /** Detect captures from text */
  detectCaptures(text: string, source: string): CaptureItem[] {
    const captures: CaptureItem[] = [];
    const seen = new Set<string>();

    for (const pattern of this.patterns) {
      for (const regex of pattern.patterns) {
        regex.lastIndex = 0;
        let match: RegExpExecArray | null;

        while ((match = regex.exec(text)) !== null) {
          // Extract the captured content
          let content = '';
          for (let i = 1; i < match.length; i++) {
            if (match[i]) {
              content = match[i]!.trim();
              break;
            }
          }

          if (!content || content.length < 5) continue;
          if (content.length > 500) content = content.slice(0, 500);

          // Deduplicate
          const key = `${pattern.type}:${content.toLowerCase()}`;
          if (seen.has(key)) continue;
          seen.add(key);

          captures.push({
            type: pattern.type,
            content,
            confidence: pattern.confidence,
            source,
          });
        }
      }
    }

    // Also check for keyword matches as fallback
    const lowerText = text.toLowerCase();
    for (const pattern of this.patterns) {
      for (const keyword of pattern.keywords) {
        if (lowerText.includes(keyword.toLowerCase())) {
          // Extract sentence containing keyword
          const sentences = text.split(/[.!?]+/);
          for (const sentence of sentences) {
            if (sentence.toLowerCase().includes(keyword.toLowerCase())) {
              const content = sentence.trim();
              if (content.length < 10 || content.length > 500) continue;

              const key = `${pattern.type}:${content.toLowerCase()}`;
              if (seen.has(key)) continue;
              seen.add(key);

              captures.push({
                type: pattern.type,
                content,
                confidence: pattern.confidence * 0.8, // Lower confidence for keyword match
                source,
              });
            }
          }
        }
      }
    }

    return captures.filter(c => c.confidence >= this.config.minConfidence);
  }

  /** Save a captured item to semantic memory */
  private async saveCapture(capture: CaptureItem): Promise<void> {
    try {
      const memory = getSemanticMemory();
      const graph = getMemoryGraph();

      const memoryId = await memory.store(
        capture.type,
        capture.content,
        {
          confidence: capture.confidence,
          source: capture.source,
          capturedBy: 'auto-capture',
          capturedAt: new Date().toISOString(),
        }
      );

      // Link to recent memories (same session context)
      const recent = await memory.getRecent(1); // Last hour
      for (const recentMem of recent.slice(0, 3)) {
        if (recentMem.id !== memoryId) {
          try {
            await graph.addEdge(memoryId, recentMem.id, 'contextual', 0.5);
          } catch {
            // Ignore edge creation errors
          }
        }
      }

      this.capturedCount++;
      this.ctx.logger.debug('Captured', {
        type: capture.type,
        preview: capture.content.slice(0, 80),
        confidence: capture.confidence,
      });
    } catch (err) {
      this.ctx.logger.error('Failed to save capture', {
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  /** Manually analyze text (for external use) */
  async analyzeText(text: string, source = 'manual'): Promise<CaptureItem[]> {
    return this.detectCaptures(text, source);
  }

  /** Get capture statistics */
  getStats(): { total: number; patterns: number } {
    return {
      total: this.capturedCount,
      patterns: this.patterns.length,
    };
  }
}

export default new AutoCaptureFeature();
