/**
 * Self-Improving Loop Types
 *
 * Types for the reflection engine that analyzes behavior,
 * creates skills from experience, and continuously improves Argentum.
 */

export interface SelfImprovingConfig {
  enabled: boolean;
  schedule: 'nightly' | 'idle' | 'both';
  nightlyTime: string; // "03:00"
  idleThreshold: number; // minutes
  skillCreationThreshold: number; // complexity score (0-10)
  maxSkillsPerRun: number;
  autoPublishToHub: boolean;
  verbose: boolean;
  dryRun: boolean;
  forceRun: boolean;
}

// ─── Phase 1: Error Analysis ────────────────────────────────────────────────

export interface ErrorAnalysis {
  id: string;
  timestamp: number;
  sessionId: string;
  errorType: 'correction' | 'failure' | 'retry' | 'timeout';
  description: string;
  rootCause: string;
  pattern: string;
  frequency: number;
  lessons: string[];
  fixSuggestion: string;
}

export interface UserCorrection {
  sessionId: string;
  timestamp: number;
  originalResponse: string;
  correctedResponse: string;
  context: string;
  category: string;
}

// ─── Phase 2: Skill Creation ────────────────────────────────────────────────

export interface SkillTemplate {
  name: string;
  description: string;
  category: string;
  triggers: string[];
  complexity: number; // 0-10
  frequency: number; // how often this pattern occurs
  content: string;
  sourceSessions: string[];
}

// ─── Phase 3: Memory Consolidation ────────────────────────────────────────────

export interface MemoryConsolidationResult {
  sessionsReviewed: number;
  insightsExtracted: number;
  memoriesUpdated: number;
  archivedSessions: number;
  ftsQueries: number;
}

// ─── Phase 4: User Model Update ─────────────────────────────────────────────

export interface UserModelUpdate {
  preferencesUpdated: boolean;
  topicsAdded: string[];
  patternsDetected: string[];
  communicationStyleChanges: Record<string, unknown>;
}

// ─── Phase 5: Self-Correction ───────────────────────────────────────────────

export interface SelfCorrection {
  area: 'soul' | 'response_strategy' | 'tools' | 'memory' | 'skills';
  whatWentWrong: string;
  whatWillChange: string;
  confidence: number;
  automaticallyApplied: boolean;
}

export interface SelfCorrectionResult {
  corrections: SelfCorrection[];
  soulUpdated: boolean;
  memoryUpdated: boolean;
}

// ─── Run Results ─────────────────────────────────────────────────────────────

export interface PhaseResult {
  phase: string;
  success: boolean;
  duration: number;
  itemsProcessed: number;
  itemsChanged: number;
  details: string[];
  errors: string[];
}

export interface SelfImprovingResult {
  phases: PhaseResult[];
  totalDuration: number;
  skillsCreated: number;
  lessonsLearned: number;
  correctionsApplied: number;
  dryRun: boolean;
  timestamp: number;
}

// ─── Lesson Log ─────────────────────────────────────────────────────────────

export interface LessonEntry {
  id: string;
  timestamp: number;
  category: 'insight' | 'knowledge_gap' | 'mistake' | 'pattern' | 'skill_created';
  title: string;
  description: string;
  source?: string;
  autoApplied: boolean;
  tags: string[];
}
