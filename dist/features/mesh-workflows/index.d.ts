/**
 * Mesh Workflows Feature
 *
 * Goal decomposition with dependency graph, parallel/sequential execution,
 * checkpoint/resume integration, and progress reporting.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/**
 * Mesh Workflows feature — goal decomposition and multi-step orchestration.
 *
 * Decomposes goals into subtasks, builds dependency graphs,
 * executes steps with checkpoint/resume, and reports progress.
 */
/** Mesh workflow configuration */
export interface MeshWorkflowsConfig {
    enabled: boolean;
    maxConcurrent: number;
    defaultTimeout: number;
    persistState: boolean;
    checkpointDir: string;
}
/** Workflow step definition */
export interface WorkflowStep {
    id: string;
    name: string;
    type: 'action' | 'condition' | 'parallel' | 'loop' | 'delay' | 'agent';
    config: Record<string, unknown>;
    dependencies: string[];
    nextSteps: string[];
    errorStep?: string;
    timeout?: number;
}
/** Workflow definition */
export interface Workflow {
    id: string;
    name: string;
    description: string;
    goal: string;
    version: string;
    steps: WorkflowStep[];
    entryStep: string;
    variables: Record<string, unknown>;
    createdAt: number;
    updatedAt: number;
}
/** Workflow execution state */
export interface WorkflowExecution {
    id: string;
    workflowId: string;
    status: 'pending' | 'running' | 'completed' | 'failed' | 'paused';
    currentStep: string;
    completedSteps: string[];
    failedSteps: string[];
    variables: Record<string, unknown>;
    stepResults: Map<string, unknown>;
    progress: number;
    startedAt: number;
    completedAt?: number;
    error?: string;
    checkpointData?: unknown;
}
/** Step handler function */
export type StepHandler = (step: WorkflowStep, context: Record<string, unknown>, execution: WorkflowExecution) => Promise<unknown>;
/** Goal decomposition result */
export interface DecomposedGoal {
    goal: string;
    subtasks: Array<{
        id: string;
        name: string;
        description: string;
        dependencies: string[];
        estimatedDuration?: number;
    }>;
    executionPlan: string[];
}
declare class MeshWorkflowsFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private workflows;
    private executions;
    private stepHandlers;
    private runningCount;
    private progressListeners;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Decompose a goal into subtasks using the agent */
    decomposeGoal(goal: string, context?: Record<string, unknown>): Promise<DecomposedGoal>;
    /** Simple goal analysis — extracts subtasks from goal description */
    private analyzeGoal;
    /** Topological sort for dependency ordering */
    private topologicalSort;
    /** Register a workflow definition */
    registerWorkflow(workflow: Omit<Workflow, 'createdAt' | 'updatedAt'>): Workflow;
    /** Create a workflow from a decomposed goal */
    createWorkflowFromGoal(goalId: string, decomposed: DecomposedGoal): Workflow;
    /** Register a step handler */
    registerStepHandler(type: string, handler: StepHandler): void;
    /** Register a progress listener */
    onProgress(listener: (exec: WorkflowExecution) => void): void;
    /** Execute a workflow */
    execute(workflowId: string, variables?: Record<string, unknown>): Promise<WorkflowExecution>;
    /** Run execution with dependency-aware scheduling */
    private runExecution;
    /** Emit progress to listeners */
    private emitProgress;
    /** Get execution status */
    getExecution(executionId: string): WorkflowExecution | undefined;
    /** Pause execution */
    pauseExecution(executionId: string): boolean;
    /** Resume paused execution */
    resumeExecution(executionId: string): Promise<boolean>;
    /** Checkpoint current execution state */
    checkpointExecution(executionId: string): unknown;
    /** Restore execution from checkpoint */
    restoreFromCheckpoint(data: Record<string, unknown>): Promise<WorkflowExecution>;
}
declare const _default: MeshWorkflowsFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map