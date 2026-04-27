/**
 * Self-Improving Loop Types
 *
 * Types for the reflection engine that analyzes behavior,
 * creates skills from experience, and continuously improves Argentum.
 */
export interface SelfImprovingConfig {
    enabled: boolean;
    schedule: 'nightly' | 'idle' | 'both';
    nightlyTime: string;
    idleThreshold: number;
    skillCreationThreshold: number;
    maxSkillsPerRun: number;
    autoPublishToHub: boolean;
    verbose: boolean;
    dryRun: boolean;
    forceRun: boolean;
}
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
export interface SkillTemplate {
    name: string;
    description: string;
    category: string;
    triggers: string[];
    complexity: number;
    frequency: number;
    content: string;
    sourceSessions: string[];
}
export interface MemoryConsolidationResult {
    sessionsReviewed: number;
    insightsExtracted: number;
    memoriesUpdated: number;
    archivedSessions: number;
    ftsQueries: number;
}
export interface UserModelUpdate {
    preferencesUpdated: boolean;
    topicsAdded: string[];
    patternsDetected: string[];
    communicationStyleChanges: Record<string, unknown>;
}
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
//# sourceMappingURL=types.d.ts.map