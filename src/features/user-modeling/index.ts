/**
 * User Modeling Feature (Honcho-style)
 *
 * Tracks user preferences and communication patterns over time.
 * Builds a model of the user's preferences for personalized responses.
 *
 * Similar to Hermes/Honcho dialectic user modeling but simplified.
 *
 * Tracks:
 * - Preferred response length (brief, medium, detailed)
 * - Language preferences
 * - Topics of interest
 * - Communication style
 * - Activity patterns
 */

import * as fs from 'fs';
import { mkdirSync, existsSync } from 'fs';
import * as path from 'path';

import type {
  FeatureModule,
  FeatureContext,
  FeatureMeta,
  HealthStatus,
} from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface UserPreferences {
  // Response style
  responseLength: 'brief' | 'medium' | 'detailed';
  formalityLevel: 'casual' | 'neutral' | 'formal';
  emojiUsage: 'minimal' | 'moderate' | 'frequent';

  // Language
  preferredLanguage: string;
  languagesSpoken: string[];

  // Interests (inferred from conversations)
  topicsOfInterest: string[];

  // Communication patterns
  communicationStyle: 'questioner' | 'directive' | 'collaborative' | 'mixed';
  prefersExplanations: boolean;
  technicalLevel: 'beginner' | 'intermediate' | 'advanced';

  // Activity patterns
  activeHours: number[]; // hours of day when user is typically active
  sessionFrequency: 'daily' | 'few-times-week' | 'weekly' | 'occasional';

  // Metadata
  firstSeen: number;
  lastUpdated: number;
  totalInteractions: number;
}

interface ConversationSample {
  timestamp: number;
  messageLength: number;
  hasQuestions: boolean;
  hasTechnicalTerms: boolean;
  language: string;
}

const DEFAULT_PREFERENCES: UserPreferences = {
  responseLength: 'medium',
  formalityLevel: 'neutral',
  emojiUsage: 'moderate',
  preferredLanguage: 'en',
  languagesSpoken: ['en'],
  topicsOfInterest: [],
  communicationStyle: 'mixed',
  prefersExplanations: true,
  technicalLevel: 'intermediate',
  activeHours: [],
  sessionFrequency: 'daily',
  firstSeen: Date.now(),
  lastUpdated: Date.now(),
  totalInteractions: 0,
};

// ─── Feature ─────────────────────────────────────────────────────────────────

class UserModelingFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'user-modeling',
    version: '0.0.4',
    description: 'Tracks user preferences and communication patterns (Honcho-style)',
    dependencies: [],
  };

  private ctx!: FeatureContext;
  private modelPath: string = '';
  private preferences: UserPreferences = { ...DEFAULT_PREFERENCES };
  private samples: ConversationSample[] = [];
  private initialized = false;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;

    // Determine paths
    const configuredWorkDir = config['workDir'];
    const workDir =
      typeof configuredWorkDir === 'string'
        ? configuredWorkDir
        : process.env.AGCLAW_WORKDIR ??
          path.join(process.env.HOME ?? '~', '.openclaw', 'workspace');

    const memoryDir = path.join(workDir, 'memory');
    if (!existsSync(memoryDir)) {
      mkdirSync(memoryDir, { recursive: true });
    }

    this.modelPath = path.join(memoryDir, 'user-modeling.md');

    this.loadModel();
    this.initialized = true;

    this.ctx.logger?.info('UserModeling initialized', {
      modelPath: this.modelPath,
      topics: this.preferences.topicsOfInterest.length,
    });
  }

  async start(): Promise<void> {
    // Inject user model into system context if available
    const modelContent = this.getModelForPrompt();
    if (modelContent) {
      this.ctx.logger?.debug('User model loaded for prompt injection');
    }
  }

  async stop(): Promise<void> {
    this.saveModel();
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: true,
      details: {
        modelPath: this.modelPath,
        totalInteractions: this.preferences.totalInteractions,
        topicsOfInterest: this.preferences.topicsOfInterest.length,
        lastUpdated: new Date(this.preferences.lastUpdated).toISOString(),
      },
    };
  }

  // ══════════════════════════════════════════════════════════════════════════
  // PUBLIC API
  // ══════════════════════════════════════════════════════════════════════════

  /**
   * Record a conversation sample to improve user model
   */
  recordSample(sample: {
    messageLength: number;
    hasQuestions?: boolean;
    hasTechnicalTerms?: boolean;
    language?: string;
    topics?: string[];
  }): void {
    const convSample: ConversationSample = {
      timestamp: Date.now(),
      messageLength: sample.messageLength,
      hasQuestions: sample.hasQuestions ?? false,
      hasTechnicalTerms: sample.hasTechnicalTerms ?? false,
      language: sample.language ?? 'en',
    };

    this.samples.push(convSample);

    // Keep only last 100 samples
    if (this.samples.length > 100) {
      this.samples = this.samples.slice(-100);
    }

    // Update preferences based on sample
    this.updatePreferencesFromSample(convSample, sample.topics);

    this.preferences.totalInteractions++;
    this.preferences.lastUpdated = Date.now();

    // Auto-save every 10 interactions
    if (this.preferences.totalInteractions % 10 === 0) {
      this.saveModel();
    }
  }

  /**
   * Get current user preferences
   */
  getPreferences(): UserPreferences {
    return { ...this.preferences };
  }

  /**
   * Get user model formatted for system prompt injection
   */
  getModelForPrompt(): string {
    if (this.preferences.totalInteractions < 3) {
      return ''; // Not enough data yet
    }

    const lines: string[] = [];
    lines.push('## User Preferences');
    lines.push('');
    lines.push(`- Response style: ${this.preferences.responseLength}`);
    lines.push(`- Formality: ${this.preferences.formalityLevel}`);
    lines.push(`- Emoji usage: ${this.preferences.emojiUsage}`);
    lines.push(`- Preferred language: ${this.preferences.preferredLanguage}`);

    if (this.preferences.topicsOfInterest.length > 0) {
      lines.push(`- Interests: ${this.preferences.topicsOfInterest.slice(0, 10).join(', ')}`);
    }

    lines.push(`- Communication style: ${this.preferences.communicationStyle}`);
    lines.push(`- Technical level: ${this.preferences.technicalLevel}`);
    lines.push(`- Prefers explanations: ${this.preferences.prefersExplanations}`);

    return lines.join('\n');
  }

  /**
   * Update specific preference manually
   */
  updatePreference<K extends keyof UserPreferences>(key: K, value: UserPreferences[K]): void {
    this.preferences[key] = value;
    this.preferences.lastUpdated = Date.now();
    this.saveModel();
  }

  /**
   * Get active hours (hours when user typically interacts)
   */
  getActiveHours(): number[] {
    if (this.samples.length < 5) return [];

    const hourCounts = new Map<number, number>();
    for (const sample of this.samples) {
      const hour = new Date(sample.timestamp).getHours();
      hourCounts.set(hour, (hourCounts.get(hour) ?? 0) + 1);
    }

    // Return hours with >1 sample
    return [...hourCounts.entries()]
      .filter(([, count]) => count > 1)
      .sort((a, b) => b[1] - a[1])
      .map(([hour]) => hour);
  }

  // ══════════════════════════════════════════════════════════════════════════
  // PRIVATE
  // ══════════════════════════════════════════════════════════════════════════

  private loadModel(): void {
    if (!existsSync(this.modelPath)) {
      this.ctx.logger?.info('No existing user model found, starting fresh');
      return;
    }

    try {
      const content = fs.readFileSync(this.modelPath, 'utf8');
      this.parseModelFile(content);
      this.ctx.logger?.info('User model loaded', {
        interactions: this.preferences.totalInteractions,
      });
    } catch (err) {
      this.ctx.logger?.warn('Failed to load user model', { error: String(err) });
    }
  }

  private saveModel(): void {
    try {
      const content = this.serializeModel();
      fs.writeFileSync(this.modelPath, content, 'utf8');
    } catch (err) {
      this.ctx.logger?.warn('Failed to save user model', { error: String(err) });
    }
  }

  private parseModelFile(content: string): void {
    // Parse YAML frontmatter if present
    const frontmatterMatch = content.match(/^---\n([\s\S]*?)\n---\n/);
    let body = content;

    if (frontmatterMatch) {
      try {
        const yaml = this.parseSimpleYaml(frontmatterMatch[1]!);

        // Validate and assign typed values
        const responseLength = yaml['responseLength'] as string | undefined;
        if (responseLength && ['brief', 'medium', 'detailed'].includes(responseLength)) {
          this.preferences.responseLength = responseLength as 'brief' | 'medium' | 'detailed';
        }

        const formalityLevel = yaml['formalityLevel'] as string | undefined;
        if (formalityLevel && ['casual', 'neutral', 'formal'].includes(formalityLevel)) {
          this.preferences.formalityLevel = formalityLevel as 'casual' | 'neutral' | 'formal';
        }

        const emojiUsage = yaml['emojiUsage'] as string | undefined;
        if (emojiUsage && ['minimal', 'moderate', 'frequent'].includes(emojiUsage)) {
          this.preferences.emojiUsage = emojiUsage as 'minimal' | 'moderate' | 'frequent';
        }

        const preferredLanguage = yaml['preferredLanguage'] as string | undefined;
        if (preferredLanguage && typeof preferredLanguage === 'string') {
          this.preferences.preferredLanguage = preferredLanguage;
        }

        const languagesSpoken = yaml['languagesSpoken'] as string[] | undefined;
        if (languagesSpoken && Array.isArray(languagesSpoken)) {
          this.preferences.languagesSpoken = languagesSpoken;
        }

        const topicsOfInterest = yaml['topicsOfInterest'] as string[] | undefined;
        if (topicsOfInterest && Array.isArray(topicsOfInterest)) {
          this.preferences.topicsOfInterest = topicsOfInterest;
        }

        const communicationStyle = yaml['communicationStyle'] as string | undefined;
        if (
          communicationStyle &&
          ['questioner', 'directive', 'collaborative', 'mixed'].includes(communicationStyle)
        ) {
          this.preferences.communicationStyle = communicationStyle as
            | 'questioner'
            | 'directive'
            | 'collaborative'
            | 'mixed';
        }

        const prefersExplanations = yaml['prefersExplanations'] as boolean | undefined;
        if (prefersExplanations !== undefined && prefersExplanations !== null) {
          this.preferences.prefersExplanations = Boolean(prefersExplanations);
        }

        const technicalLevel = yaml['technicalLevel'] as string | undefined;
        if (technicalLevel && ['beginner', 'intermediate', 'advanced'].includes(technicalLevel)) {
          this.preferences.technicalLevel = technicalLevel as
            | 'beginner'
            | 'intermediate'
            | 'advanced';
        }

        const activeHours = yaml['activeHours'] as number[] | undefined;
        if (activeHours && Array.isArray(activeHours)) {
          this.preferences.activeHours = activeHours;
        }

        const sessionFrequency = yaml['sessionFrequency'] as string | undefined;
        if (
          sessionFrequency &&
          ['daily', 'few-times-week', 'weekly', 'occasional'].includes(sessionFrequency)
        ) {
          this.preferences.sessionFrequency = sessionFrequency as
            | 'daily'
            | 'few-times-week'
            | 'weekly'
            | 'occasional';
        }

        const firstSeen = yaml['firstSeen'] as number | undefined;
        if (typeof firstSeen === 'number') {
          this.preferences.firstSeen = firstSeen;
        }

        const lastUpdated = yaml['lastUpdated'] as number | undefined;
        if (typeof lastUpdated === 'number') {
          this.preferences.lastUpdated = lastUpdated;
        }

        const totalInteractions = yaml['totalInteractions'] as number | undefined;
        if (typeof totalInteractions === 'number') {
          this.preferences.totalInteractions = totalInteractions;
        }
      } catch (err) {
        this.ctx.logger?.warn('Failed to parse user model frontmatter', { error: String(err) });
      }
      body = content.slice(frontmatterMatch[0].length);
    }

    // Parse topic suggestions from body
    const topicMatches = body.match(
      /#{1,2}\s*(?:topics?|interests?|learned about):?\s*([^\n#-]+)/gi,
    );
    if (topicMatches) {
      for (const match of topicMatches) {
        const topics = match
          .replace(/#{1,2}\s*(?:topics?|interests?|learned about):?\s*/i, '')
          .split(/[,;]/)
          .map((t) => t.trim().toLowerCase())
          .filter((t) => t.length > 2);
        for (const topic of topics) {
          if (!this.preferences.topicsOfInterest.includes(topic)) {
            this.preferences.topicsOfInterest.push(topic);
          }
        }
      }
    }
  }

  private serializeModel(): string {
    const fm = [
      '---',
      `responseLength: ${this.preferences.responseLength}`,
      `formalityLevel: ${this.preferences.formalityLevel}`,
      `emojiUsage: ${this.preferences.emojiUsage}`,
      `preferredLanguage: ${this.preferences.preferredLanguage}`,
      `languagesSpoken: [${this.preferences.languagesSpoken.join(', ')}]`,
      `topicsOfInterest: [${this.preferences.topicsOfInterest.join(', ')}]`,
      `communicationStyle: ${this.preferences.communicationStyle}`,
      `prefersExplanations: ${this.preferences.prefersExplanations}`,
      `technicalLevel: ${this.preferences.technicalLevel}`,
      `activeHours: [${this.preferences.activeHours.join(', ')}]`,
      `sessionFrequency: ${this.preferences.sessionFrequency}`,
      `firstSeen: ${this.preferences.firstSeen}`,
      `lastUpdated: ${this.preferences.lastUpdated}`,
      `totalInteractions: ${this.preferences.totalInteractions}`,
      '---',
      '',
      '# User Modeling Data',
      '',
      'This file tracks user preferences inferred from conversation patterns.',
      'Do not edit manually unless necessary.',
      '',
    ].join('\n');

    return fm;
  }

  private parseSimpleYaml(yaml: string): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    const lines = yaml.split('\n');

    for (const line of lines) {
      const colonIdx = line.indexOf(':');
      if (colonIdx === -1) continue;

      const key = line.slice(0, colonIdx).trim();
      const rawValue = line.slice(colonIdx + 1).trim();

      if (!key) continue;

      // Handle arrays
      if (rawValue.startsWith('[') && rawValue.endsWith(']')) {
        const arrContent = rawValue.slice(1, -1).trim();
        result[key] = arrContent
          ? arrContent
              .split(',')
              .map((s) => s.trim())
              .filter((s) => s.length > 0)
          : [];
      } else if (rawValue === 'true') {
        result[key] = true;
      } else if (rawValue === 'false') {
        result[key] = false;
      } else if (rawValue !== '' && !isNaN(Number(rawValue))) {
        result[key] = Number(rawValue);
      } else if (rawValue !== '') {
        result[key] = rawValue;
      }
    }

    return result;
  }

  private updatePreferencesFromSample(sample: ConversationSample, topics?: string[]): void {
    // Update response length based on message length
    if (sample.messageLength < 100) {
      this.preferences.responseLength = 'brief';
    } else if (sample.messageLength < 500) {
      if (this.preferences.responseLength === 'brief') {
        this.preferences.responseLength = 'medium';
      }
    } else {
      this.preferences.responseLength = 'detailed';
    }

    // Update technical level
    if (sample.hasTechnicalTerms) {
      this.preferences.technicalLevel = 'advanced';
    }

    // Update topics of interest
    if (topics && topics.length > 0) {
      for (const topic of topics) {
        const normalized = topic.toLowerCase().trim();
        if (normalized.length > 2 && !this.preferences.topicsOfInterest.includes(normalized)) {
          this.preferences.topicsOfInterest.push(normalized);
        }
      }
      // Keep only top 20 topics
      if (this.preferences.topicsOfInterest.length > 20) {
        this.preferences.topicsOfInterest = this.preferences.topicsOfInterest.slice(-20);
      }
    }

    // Update communication style
    if (sample.hasQuestions) {
      this.preferences.communicationStyle = 'questioner';
    }

    // Update active hours
    const hour = new Date(sample.timestamp).getHours();
    if (!this.preferences.activeHours.includes(hour)) {
      this.preferences.activeHours.push(hour);
      if (this.preferences.activeHours.length > 14) {
        this.preferences.activeHours = this.preferences.activeHours.slice(-14);
      }
    }
  }
}

export default new UserModelingFeature();
