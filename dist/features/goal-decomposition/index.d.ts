/**
 * Goal Decomposition Feature (SQLite)
 *
 * Stores goals and tasks with parent-child relationships and dependencies.
 */
import type { FeatureModule, FeatureMeta, FeatureContext, HealthStatus } from '../../core/plugin-loader';
export type TaskStatus = 'pending' | 'in-progress' | 'done' | 'blocked';
export interface TaskRow {
    id: string;
    goal_id: string;
    parent_id: string | null;
    title: string;
    description: string;
    status: TaskStatus;
    priority: number;
    created_at: number;
    updated_at: number;
}
export interface GoalDecompositionConfig {
    enabled: boolean;
    dbPath: string;
}
declare class GoalDecompositionFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    createGoal(title: string, description?: string): string;
    addSubTask(goalId: string, title: string, description?: string, parentId?: string, priority?: number): string;
    setDependency(taskId: string, dependsOnId: string): void;
    updateStatus(taskId: string, status: TaskStatus): void;
    getTaskTree(goalId: string): TaskRow | null;
    getReadyTasks(): TaskRow[];
    private initDatabase;
}
declare const _default: GoalDecompositionFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map