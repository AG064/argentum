import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export type AgentStatus = 'idle' | 'busy' | 'offline';
export interface AgentInfo {
    id: string;
    name: string;
    capabilities: string[];
    status: AgentStatus;
    currentTaskId: string | null;
    lastSeen: number;
    metadata: Record<string, unknown>;
}
export interface Task {
    taskId: string;
    assignedAgentId: string | null;
    status: 'pending' | 'assigned' | 'running' | 'completed' | 'failed';
    createdAt: number;
    assignedAt: number | null;
    completedAt: number | null;
    payload: Record<string, unknown>;
    result: Record<string, unknown> | null;
}
export interface MultiAgentCoordinationConfig {
    enabled: boolean;
    dbPath: string;
    heartbeatIntervalMs: number;
    offlineTimeoutMs: number;
    maxAgents: number;
}
declare class MultiAgentCoordinationFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private db;
    private ctx;
    private agents;
    private heartbeatTimer;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Register a new agent or update existing one */
    registerAgent(id: string, name: string, capabilities: string[]): Promise<void>;
    /** Unregister an agent */
    unregisterAgent(id: string): Promise<boolean>;
    /** Get agent info */
    getAgent(id: string): Promise<AgentInfo | null>;
    /** Get all agents */
    getAvailableAgents(): AgentInfo[];
    /** Get agents by capability */
    getAgentsByCapability(capability: string): AgentInfo[];
    /** Update agent's last seen timestamp */
    heartbeat(agentId: string): Promise<boolean>;
    /** Check agents that haven't sent heartbeat and mark them offline */
    private checkOfflineAgents;
    private updateAgentStatusesToOffline;
    /** Assign a task to an agent */
    assignTask(taskId: string, agentId: string, payload?: Record<string, unknown>): Promise<boolean>;
    /** Complete a task */
    completeTask(taskId: string, result?: Record<string, unknown>): Promise<boolean>;
    /** Fail a task */
    failTask(taskId: string): Promise<boolean>;
    /** Get task by ID */
    getTask(taskId: string): Promise<Task | null>;
    /** List tasks by status */
    listTasks(status?: string): Promise<Task[]>;
    private mapTaskRow;
    /** Broadcast a message to all agents (or filtered by capability) */
    broadcast(message: string, capability?: string, excludeAgentId?: string): Promise<number>;
    private initDatabase;
    private insertAgentToDb;
    private upsertAgentToDb;
    private updateAgentInDb;
    private loadAgentsFromDb;
    private insertTaskToDb;
}
declare const _default: MultiAgentCoordinationFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map