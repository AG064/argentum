/**
 * Trajectory Export Types
 *
 * Interfaces for conversation trajectory recording and JSONL export.
 * Designed for RL fine-tuning data preparation.
 */
export interface TrajectoryMessage {
    role: 'user' | 'assistant' | 'system';
    content: string;
    toolCalls?: ToolCall[];
    tools?: string[];
}
export interface ToolCall {
    id: string;
    name: string;
    arguments: Record<string, unknown>;
    result?: string;
}
export interface TrajectoryMetadata {
    model: string;
    tokens: number;
    cost: number;
    agentId?: string;
    tags: string[];
}
export interface TrajectoryEntry {
    timestamp: string;
    sessionId: string;
    messages: TrajectoryMessage[];
    metadata: TrajectoryMetadata;
}
export interface TrajectoryExportOptions {
    sessionId?: string;
    since?: Date;
    until?: Date;
    agentId?: string;
    tags?: string[];
    gzip?: boolean;
    format?: 'jsonl' | 'json';
}
export interface TrajectoryStats {
    totalSessions: number;
    totalMessages: number;
    totalTokens: number;
    totalCost: number;
    byAgent: Record<string, {
        sessions: number;
        messages: number;
        tokens: number;
        cost: number;
    }>;
    byTag: Record<string, {
        sessions: number;
        messages: number;
    }>;
}
export interface TrajectoryConfig {
    enabled: boolean;
    dbPath: string;
    compressExports: boolean;
    defaultFormat: 'jsonl' | 'json';
}
//# sourceMappingURL=types.d.ts.map