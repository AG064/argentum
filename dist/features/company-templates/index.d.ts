/**
 * Company Templates Feature
 *
 * Export and import company/organization configurations as portable JSON bundles.
 * Scrubs secrets automatically. Supports versioned template bundles.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface TemplateBundle {
    version: string;
    name: string;
    exportedAt: string;
    organization: OrganizationConfig;
    agents: AgentConfig[];
    goals: GoalConfig[];
    skills: SkillConfig[];
    workflows: WorkflowConfig[];
    metadata: Record<string, unknown>;
}
export interface OrganizationConfig {
    name: string;
    timezone: string;
    locale: string;
    settings: Record<string, unknown>;
}
export interface AgentConfig {
    name: string;
    role: string;
    model: string;
    systemPrompt: string;
    tools: string[];
    enabled: boolean;
}
export interface GoalConfig {
    title: string;
    description: string;
    status: string;
    metrics: Record<string, unknown>;
}
export interface SkillConfig {
    name: string;
    version: string;
    enabled: boolean;
    config: Record<string, unknown>;
}
export interface WorkflowConfig {
    name: string;
    trigger: string;
    steps: Array<Record<string, unknown>>;
    enabled: boolean;
}
export interface TemplateInfo {
    name: string;
    version: string;
    exportedAt: string;
    size: number;
    agentCount: number;
    goalCount: number;
}
declare class CompanyTemplatesFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private templates;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Export a company configuration as a portable template bundle */
    exportCompany(name: string): Promise<TemplateBundle>;
    /** Import a company template bundle */
    importCompany(bundle: TemplateBundle, name: string): Promise<void>;
    /** List all saved templates */
    listTemplates(): Promise<TemplateInfo[]>;
    /** Get a specific template bundle */
    getTemplate(name: string): Promise<TemplateBundle | null>;
    /** Delete a template */
    deleteTemplate(name: string): Promise<boolean>;
    /** Export template to a portable file path */
    exportToFile(name: string, outputPath: string): Promise<void>;
    /** Import template from a file path */
    importFromFile(filePath: string, name?: string): Promise<void>;
    private ensureTemplatesDir;
    private loadExistingTemplates;
    private sanitizeFileName;
    private extractAgents;
    private extractSkills;
}
declare const _default: CompanyTemplatesFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map