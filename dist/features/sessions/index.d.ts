/**
 * Argentum Sessions
 *
 * Session management for conversations and agent interactions.
 * Stores messages, tracks context, manages history.
 */
import { type FeatureContext } from '../../core/plugin-loader';
interface Session {
    id: string;
    title: string;
    createdAt: number;
    updatedAt: number;
    messageCount: number;
    model: string;
    status: 'active' | 'archived' | 'deleted';
    tags: string[];
    metadata: Record<string, unknown>;
}
interface Message {
    id: string;
    sessionId: string;
    role: 'user' | 'assistant' | 'system' | 'tool';
    content: string;
    timestamp: number;
    toolCalls?: Array<{
        name: string;
        arguments: string;
        result?: string;
    }>;
    metadata?: Record<string, unknown>;
}
declare class SessionsFeature {
    readonly meta: {
        name: string;
        version: string;
        description: string;
        dependencies: string[];
    };
    private _ctx;
    private db;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<{
        healthy: boolean;
        details: Record<string, unknown>;
    }>;
    create(title?: string, model?: string): Session;
    get(id: string): Session | null;
    list(options?: {
        status?: string;
        limit?: number;
        offset?: number;
    }): Session[];
    update(id: string, updates: Partial<Pick<Session, 'title' | 'status' | 'tags'>>): boolean;
    archive(id: string): boolean;
    delete(id: string): boolean;
    addMessage(sessionId: string, role: Message['role'], content: string, toolCalls?: Message['toolCalls']): Message;
    getMessages(sessionId: string, options?: {
        limit?: number;
        offset?: number;
        role?: Message['role'];
    }): Message[];
    search(query: string, limit?: number): Array<{
        sessionId: string;
        messageId: string;
        content: string;
        role: string;
        timestamp: number;
    }>;
    setState(sessionId: string, key: string, value: string): void;
    getState(sessionId: string, key: string): string | null;
    getAllState(sessionId: string): Record<string, string>;
    export(sessionId: string): {
        session: Session;
        messages: Message[];
        state: Record<string, string>;
    } | null;
    getStats(): {
        totalSessions: number;
        activeSessions: number;
        totalMessages: number;
        avgMessagesPerSession: number;
    };
    private get database();
    private mapSessionRow;
}
declare const _default: SessionsFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map