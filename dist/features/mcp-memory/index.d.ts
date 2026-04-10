export interface MCPConfig {
    enabled: boolean;
    provider?: 'local' | 'remote';
    endpoint?: string;
}
export interface MemoryResult {
    success: boolean;
    data?: string;
    error?: string;
}
export declare class MCPMemoryFeature {
    private logger;
    private config;
    constructor(config: MCPConfig);
    store(key: string, value: string): Promise<MemoryResult>;
    retrieve(key: string): Promise<MemoryResult>;
    search(query: string): Promise<MemoryResult>;
    healthCheck(): {
        healthy: boolean;
        message: string;
    };
}
declare const _default: MCPMemoryFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map