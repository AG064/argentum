/**
 * Checkpoint Feature
 *
 * OMEGA Memory integration — save and restore task state between sessions.
 * Integrates with mesh-workflows for persistent workflow state.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Checkpoint configuration */
export interface CheckpointConfig {
    enabled: boolean;
    maxCheckpoints: number;
    autoCheckpoint: boolean;
    checkpointIntervalMs: number;
}
/** Checkpoint entry */
export interface Checkpoint {
    taskId: string;
    state: unknown;
    context: Record<string, unknown>;
    createdAt: number;
    updatedAt: number;
}
/** Checkpoint summary for listing */
export interface CheckpointSummary {
    taskId: string;
    contextPreview: string;
    createdAt: string;
    updatedAt: string;
    stateSize: number;
}
/**
 * Checkpoint — persistent task state management.
 *
 * Save and resume task state across sessions. Integrates with
 * mesh-workflows to persist workflow execution state.
 */
declare class CheckpointFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private activeTasks;
    private timer;
    private checkpointCount;
    private resumeCount;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Save a checkpoint */
    checkpoint(taskId: string, state: unknown, context?: Record<string, unknown>): Promise<void>;
    /** Resume a checkpointed task */
    resume(taskId: string): Promise<{
        state: unknown;
        context: Record<string, unknown>;
    } | null>;
    /** List all checkpoints */
    listCheckpoints(): Promise<CheckpointSummary[]>;
    /** Delete a checkpoint */
    deleteCheckpoint(taskId: string): Promise<boolean>;
    /** Get a specific checkpoint */
    getCheckpoint(taskId: string): Promise<Checkpoint | null>;
    /** Handle task start hook */
    private handleTaskStart;
    /** Handle task update hook */
    private handleTaskUpdate;
    /** Handle task complete hook */
    private handleTaskComplete;
    /** Auto-checkpoint all active tasks */
    private autoCheckpointAll;
    /** Load active checkpoints on startup */
    private loadActiveCheckpoints;
    /** Extract preview from context JSON */
    private getContextPreview;
    /** Safe JSON parse */
    private parseJson;
}
declare const _default: CheckpointFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map