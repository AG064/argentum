/**
 * Goal Hierarchy Feature
 *
 * Tree-structured goal management with task linking and progress tracking.
 * Supports parent-child relationships and metrics.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface Goal {
    id: string;
    title: string;
    description: string;
    parentId: string | null;
    status: 'active' | 'completed' | 'paused';
    metrics: Record<string, unknown>;
    createdAt: number;
    updatedAt: number;
}
export interface GoalNode extends Goal {
    children: GoalNode[];
    taskCount: number;
    completedTasks: number;
}
export interface ProgressReport {
    goalId: string;
    title: string;
    status: string;
    totalTasks: number;
    completedTasks: number;
    subGoals: number;
    percentComplete: number;
    estimatedCompletion: string | null;
    metrics: Record<string, unknown>;
}
export interface TaskGoalLink {
    taskId: string;
    goalId: string;
    linkedAt: number;
}
declare class GoalsFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private db;
    private ctx;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Create a new goal */
    createGoal(title: string, description: string, parentId?: string): Promise<Goal>;
    /** Link a task to a goal */
    linkTaskToGoal(taskId: string, goalId: string): Promise<void>;
    /** Get full goal tree */
    getGoalTree(): Promise<GoalNode[]>;
    /** Get progress report for a goal */
    getGoalProgress(goalId: string): Promise<ProgressReport>;
    /** Update goal status */
    updateGoalStatus(goalId: string, status: Goal['status']): Promise<void>;
    /** Update goal metrics */
    updateGoalMetrics(goalId: string, metrics: Record<string, unknown>): Promise<void>;
    /** Get a single goal by ID */
    getGoal(goalId: string): Promise<Goal | null>;
    /** List all goals */
    listGoals(status?: Goal['status']): Promise<Goal[]>;
    private initDatabase;
    private getTaskCounts;
    private parseJson;
}
declare const _default: GoalsFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map