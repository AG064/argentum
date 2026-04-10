/**
 * AG-Claw Blueprint System
 *
 * YAML/JSON-driven security configuration.
 * Loads and validates blueprints from:
 * - ~/.ag-claw/blueprint.yaml
 * - ~/.ag-claw/blueprint.json
 * - AGCLAW_BLUEPRINT_PATH env var
 * - Programmatic API
 *
 * Blueprint schema:
 *   version: "1.0"
 *   sandbox:
 *     enabled: true
 *     allowedPaths: [...]
 *     deniedPaths: [...]
 *     maxMemory: 512
 *     networkIsolation: true
 *   policies:
 *     - name: "allow-read-home"
 *       resource: "file://~/.ag-claw/**"
 *       action: "read"
 *       effect: "allow"
 *   credentials:
 *     autoRotate: true
 *     ttlSeconds: 1800
 *   approval:
 *     criticalActions:
 *       - "exec://sudo"
 *       - "network://external"
 *       - "file://delete"
 */
import type { Blueprint } from '../types';
export declare class BlueprintLoader {
    private blueprint;
    private loadedPath;
    private logger;
    constructor();
    /**
     * Load blueprint from a YAML or JSON file.
     */
    loadFromFile(filePath?: string): Blueprint | null;
    /**
     * Load blueprint from environment variable (YAML or JSON string).
     */
    loadFromEnv(): Blueprint | null;
    /**
     * Set blueprint programmatically.
     */
    set(blueprint: Blueprint): void;
    /**
     * Get the current blueprint.
     */
    get(): Blueprint | null;
    /**
     * Apply the blueprint to all security components.
     */
    apply(): void;
    /**
     * Generate a default blueprint YAML.
     */
    static generateDefaultBlueprint(): string;
}
export declare function getBlueprintLoader(): BlueprintLoader;
//# sourceMappingURL=index.d.ts.map