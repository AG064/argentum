"use strict";
/**
 * skill-evolution/index.ts
 *
 * Bridge between Argentum and OpenSpace self-evolving skills engine.
 * OpenSpace: https://github.com/AG064/OpenSpace (AG064 fork)
 *
 * Provides:
 * - AUTO-FIX: repair broken SKILL.md
 * - AUTO-IMPROVE: evolve successful skills
 * - AUTO-LEARN: capture new skills from workflows
 *
 * LLM: MiniMax (primary), OpenRouter (fallback), any OpenAI-compatible API
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.SkillEvolution = void 0;
exports.createSkillEvolution = createSkillEvolution;
const child_process_1 = require("child_process");
const fs_1 = require("fs");
const os_1 = require("os");
const path_1 = require("path");
class SkillEvolution {
    openSpacePath;
    config;
    constructor(config = {}) {
        const home = (0, os_1.homedir)();
        this.openSpacePath = config.openSpacePath ?? (0, path_1.join)(home, 'OpenSpace');
        this.config = {
            openSpacePath: this.openSpacePath,
            llmProvider: config.llmProvider ?? 'openrouter',
            apiKey: config.apiKey ?? process.env.OPENROUTER_API_KEY ?? '',
            llmEndpoint: config.llmEndpoint ?? '',
            skillsDir: config.skillsDir ?? (0, path_1.join)(home, '.openclaw', 'skills'),
        };
    }
    /**
     * Diagnose a skill - check if it's healthy, broken, or needs improvement
     */
    async diagnose(skillName) {
        const result = await this.runOpenSpace(['skill-diagnose', skillName]);
        // Parse output - OpenSpace outputs JSON
        try {
            return JSON.parse(result);
        }
        catch {
            // If not JSON, try text parsing
            return {
                skill: skillName,
                health: result.includes('ERROR') ? 'broken' : result.includes('WARNING') ? 'needs-improvement' : 'healthy',
                issues: result.includes('ERROR') ? [result] : [],
                suggestions: result.includes('WARNING') ? [result] : [],
            };
        }
    }
    /**
     * AUTO-FIX: Repair a broken skill
     */
    async fixSkill(skillPath) {
        console.log(`[SkillEvolution] AUTO-FIX: repairing ${skillPath}`);
        const result = await this.runOpenSpace(['skill-fix', skillPath]);
        if (result.includes('ERROR')) {
            return { success: false, changes: [], error: result };
        }
        return {
            success: true,
            changes: result.split('\n').filter(line => line.includes('FIXED') || line.includes('Changed')),
        };
    }
    /**
     * AUTO-IMPROVE: Evolve a skill based on usage patterns
     */
    async evolveSkill(skillName, metrics) {
        console.log(`[SkillEvolution] AUTO-IMPROVE: evolving ${skillName}`);
        let result;
        if (metrics) {
            const metricsFile = `/tmp/skill-metrics-${Date.now()}.json`;
            const fs = await Promise.resolve().then(() => __importStar(require('fs')));
            fs.writeFileSync(metricsFile, JSON.stringify(metrics));
            result = await this.runOpenSpace(['skill-evolve', skillName, '--metrics', metricsFile]);
        }
        else {
            result = await this.runOpenSpace(['skill-evolve', skillName]);
        }
        try {
            return JSON.parse(result);
        }
        catch {
            return {
                skill: skillName,
                originalVersion: 1,
                newVersion: 2,
                changes: result.split('\n'),
                improved: !result.includes('ERROR'),
            };
        }
    }
    /**
     * AUTO-LEARN: Capture a new skill from successful execution
     */
    async learnFromWorkflow(executionLog, options) {
        console.log(`[SkillEvolution] AUTO-LEARN: capturing from workflow`);
        const logFile = `/tmp/workflow-${Date.now()}.log`;
        const fs = await Promise.resolve().then(() => __importStar(require('fs')));
        fs.writeFileSync(logFile, executionLog);
        const result = await this.runOpenSpace([
            'skill-learn',
            '--from', logFile,
            '--auto-install', options?.autoInstall ? 'true' : 'false'
        ]);
        try {
            return JSON.parse(result);
        }
        catch {
            return {
                newSkillName: 'unknown',
                capturedFrom: logFile,
                confidence: 0,
                autoInstall: false,
            };
        }
    }
    /**
     * Run OpenSpace CLI command
     */
    async runOpenSpace(args) {
        // Build env with MiniMax/OpenAI-compatible LLM
        const env = {
            ...process.env,
            // LiteLLM reads these for minimax/MiniMax-M2.7 calls
            MINIMAX_API_KEY: this.config.apiKey,
            MINIMAX_API_BASE: 'https://api.minimax.io/v1',
            MINIMAX_MODEL: 'MiniMax-M2.7',
            // OpenSpace model selection
            OPENSPACE_MODEL: 'minimax/MiniMax-M2.7',
        };
        if (this.config.llmEndpoint) {
            env.LITELLM_ENDPOINT = this.config.llmEndpoint;
        }
        return new Promise((resolve, reject) => {
            const proc = (0, child_process_1.spawn)('openspace', args, {
                cwd: this.openSpacePath,
                env,
                timeout: 60000,
            });
            let stdout = '';
            let stderr = '';
            proc.stdout.on('data', (data) => { stdout += data.toString(); });
            proc.stderr.on('data', (data) => { stderr += data.toString(); });
            proc.on('close', (code) => {
                if (code === 0) {
                    resolve(stdout);
                }
                else {
                    resolve(stderr || `Exit code: ${code}`);
                }
            });
            proc.on('error', (err) => {
                reject(err);
            });
        });
    }
    /**
     * Check if OpenSpace is installed and configured
     */
    async isConfigured() {
        try {
            await this.runOpenSpace(['--version']);
            return { available: true };
        }
        catch (err) {
            return {
                available: false,
                error: 'OpenSpace not installed. Run: pip install -e ~/OpenSpace'
            };
        }
    }
}
exports.SkillEvolution = SkillEvolution;
/**
 * Create SkillEvolution instance from Argentum credentials
 * Reads API keys from ~/.openclaw/credentials/telegram.json
 */
async function createSkillEvolution() {
    const credsPath = (0, path_1.join)((0, os_1.homedir)(), '.openclaw', 'credentials', 'telegram.json');
    let minimaxKey = '';
    if ((0, fs_1.existsSync)(credsPath)) {
        try {
            const creds = JSON.parse((0, fs_1.readFileSync)(credsPath, 'utf-8'));
            minimaxKey = creds.minimax ?? '';
        }
        catch {
            // Use env vars
        }
    }
    return new SkillEvolution({
        llmProvider: 'minimax',
        apiKey: minimaxKey || process.env.MINIMAX_API_KEY || '',
    });
}
//# sourceMappingURL=index.js.map