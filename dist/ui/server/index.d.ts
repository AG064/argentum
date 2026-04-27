/**
 * Argentum Dashboard Server
 *
 * A lightweight secure web server for the Argentum dashboard.
 * Features:
 * - HTTP Basic Auth
 * - Static file serving
 * - WebSocket for real-time updates
 * - Rate limiting
 * - CORS configuration
 */
import * as http from 'http';
interface AuthConfig {
    username: string;
    passwordHash: string;
}
interface ServerConfig {
    port: number;
    host: string;
    staticDir: string;
    auth: AuthConfig;
    rateLimit: {
        windowMs: number;
        maxRequests: number;
    };
    cors: {
        enabled: boolean;
        allowedOrigins: string[];
    };
}
/**
 * Broadcast event to all WebSocket clients
 */
export declare function broadcast(type: string, data: unknown): void;
/**
 * Start the dashboard server
 */
export declare function startDashboardServer(options?: Partial<ServerConfig>): Promise<http.Server>;
/**
 * Stop the dashboard server
 */
export declare function stopDashboardServer(server: http.Server): void;
export {};
//# sourceMappingURL=index.d.ts.map