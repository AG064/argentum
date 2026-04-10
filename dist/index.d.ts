/**
 * AG-Claw Entry Point
 *
 * Bootstraps the AG-Claw agent framework:
 * - Loads configuration
 * - Initializes logging
 * - Starts the plugin loader
 * - Enables configured features
 * - Implements Agentic Tool Loop
 * - Sets up graceful shutdown
 */
import 'dotenv/config';
import { type AGClawConfig } from './core/config';
import { type LLMProvider, type Message } from './core/llm-provider';
import { type MemoryGraph } from './memory/graph';
import { type SemanticMemory } from './memory/semantic';
export interface Tool {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
    execute: (params: Record<string, unknown>) => Promise<string>;
}
export declare class Agent {
    private tools;
    private model;
    private logger;
    private maxIterations;
    private systemPrompt;
    constructor(model: LLMProvider, systemPrompt?: string);
    registerTool(tool: Tool): void;
    unregisterTool(name: string): void;
    getToolNames(): string[];
    handleMessage(message: string, conversationHistory?: Message[]): Promise<string>;
}
export interface MemoryBackend {
    store(entry: MemoryEntry): Promise<void>;
    search(query: string, limit?: number): Promise<MemoryEntry[]>;
    getRecent(limit?: number): Promise<MemoryEntry[]>;
    delete(id: string): Promise<boolean>;
}
export interface MemoryEntry {
    id: string;
    content: string;
    createdAt: number;
    accessedAt: number;
    accessCount: number;
    metadata?: Record<string, unknown>;
}
declare class AGClaw {
    private config;
    private logger;
    private pluginLoader;
    private agent;
    private llmProvider;
    private shuttingDown;
    private semanticMemory;
    private memoryGraph;
    constructor();
    /** Get the agent instance (for channels to use) */
    getAgent(): Agent;
    /** Get the config */
    getConfig(): AGClawConfig;
    /** Get the semantic memory instance */
    getSemanticMemory(): SemanticMemory;
    /** Get the memory graph instance */
    getMemoryGraph(): MemoryGraph;
    /** Start the AG-Claw framework */
    start(): Promise<void>;
    /** Start configured channels (Telegram, Webchat) */
    private startChannels;
    /** Start Telegram bot using grammY */
    private startTelegram;
    /** Periodic health checks for active features */
    private startHealthChecks;
    /** Register OMEGA Memory tools for the agent */
    private registerMemoryTools;
    /** Register signal handlers for graceful shutdown */
    private registerShutdownHandlers;
    /** Graceful shutdown */
    private shutdown;
}
export declare function getAGClaw(): AGClaw;
export { AGClaw };
//# sourceMappingURL=index.d.ts.map