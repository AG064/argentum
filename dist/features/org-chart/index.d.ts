/**
 * Org Chart Feature
 *
 * Manages organizational hierarchy where Argentum is CEO and subagents are team members.
 * Provides hire/fire/assign operations and system prompt injection.
 *
 * Argentum as CEO injects org context:
 *   ## Your Team
 *   - CTO (coder agent): Handles all code tasks
 *   - Researcher (researcher agent): Handles research
 *   - Analyst: Handles data analysis
 *
 *   You can delegate tasks to them using subagent spawning.
 */
import { type OrgNode, type TaskAssignment, type OrgStats, type OrgRole } from './types';
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
declare class OrgChartFeature implements FeatureModule {
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
     * Get all org nodes as a flat list.
     */
    getAllNodes(): OrgNode[];
    /**
     * Get org nodes in tree format (for display).
     */
    getTree(): OrgNode[];
    /**
     * Get active agents only.
     */
    getActiveAgents(): OrgNode[];
    /**
     * Get a single node by ID.
     */
    getNode(id: string): OrgNode | null;
    /**
     * Hire a new agent/subagent.
     */
    hire(node: Omit<OrgNode, 'hiredAt' | 'status'>): OrgNode;
    /**
     * Terminate an agent.
     */
    fire(id: string): boolean;
    /**
     * Pause an agent (can be resumed).
     */
    pause(id: string): boolean;
    /**
     * Resume a paused agent.
     */
    resume(id: string): boolean;
    /**
     * Assign a task to an agent.
     */
    assignTask(task: Omit<TaskAssignment, 'assignedAt'>): TaskAssignment;
    /**
     * Get tasks assigned to an agent.
     */
    getAgentTasks(agentId: string): TaskAssignment[];
    /**
     * Update task status.
     */
    updateTaskStatus(taskId: string, status: TaskAssignment['status']): boolean;
    /**
     * Generate system prompt section describing the team.
     */
    getTeamContext(): string;
    /**
     * Get organization statistics.
     */
    getStats(): OrgStats;
    /**
     * Pretty-print the org chart as an ASCII tree.
     */
    printTree(): string;
    /**
     * Generate default config for a new hire based on role.
     */
    getDefaultConfig(role: OrgRole): Partial<OrgNode>;
    private initDatabase;
    private ensureCEO;
}
declare const _default: OrgChartFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map