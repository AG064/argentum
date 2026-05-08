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

import { spawn } from 'child_process';
import { existsSync, readFileSync } from 'fs';
import { homedir } from 'os';
import { join, dirname } from 'path';

export interface SkillEvolutionConfig {
  /** Path to OpenSpace installation (default: ~/OpenSpace) */
  openSpacePath?: string;
  /** LLM provider: 'minimax' | 'openrouter' | 'nvidia' | 'local' */
  llmProvider?: 'minimax' | 'openrouter' | 'nvidia' | 'local';
  /** API key for the LLM provider */
  apiKey?: string;
  /** Custom LLM endpoint (for local models) */
  llmEndpoint?: string;
  /** Skills directory to monitor */
  skillsDir?: string;
}

export interface DiagnoseResult {
  skill: string;
  health: 'healthy' | 'broken' | 'needs-improvement';
  issues: string[];
  suggestions: string[];
}

export interface EvolveResult {
  skill: string;
  originalVersion: number;
  newVersion: number;
  changes: string[];
  improved: boolean;
}

export interface LearnResult {
  newSkillName: string;
  capturedFrom: string;
  confidence: number;
  autoInstall: boolean;
}

function firstNonEmpty(...values: Array<string | undefined>): string {
  return values.find((value) => value !== undefined && value.length > 0) ?? '';
}

export class SkillEvolution {
  private openSpacePath: string;
  private config: Required<SkillEvolutionConfig>;

  constructor(config: SkillEvolutionConfig = {}) {
    const home = homedir();
    
    this.openSpacePath = config.openSpacePath ?? join(home, 'OpenSpace');
    this.config = {
      openSpacePath: this.openSpacePath,
      llmProvider: config.llmProvider ?? 'openrouter',
      apiKey: config.apiKey ?? process.env.OPENROUTER_API_KEY ?? '',
      llmEndpoint: config.llmEndpoint ?? '',
      skillsDir: config.skillsDir ?? join(home, '.openclaw', 'skills'),
    };
  }

  /**
   * Diagnose a skill - check if it's healthy, broken, or needs improvement
   */
  async diagnose(skillName: string): Promise<DiagnoseResult> {
    const result = await this.runOpenSpace(['skill-diagnose', skillName]);
    
    // Parse output - OpenSpace outputs JSON
    try {
      return JSON.parse(result);
    } catch {
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
  async fixSkill(skillPath: string): Promise<{ success: boolean; changes: string[]; error?: string }> {
    console.info(`[SkillEvolution] AUTO-FIX: repairing ${skillPath}`);
    
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
  async evolveSkill(skillName: string, metrics?: Record<string, number>): Promise<EvolveResult> {
    console.info(`[SkillEvolution] AUTO-IMPROVE: evolving ${skillName}`);
    
    let result: string;
    if (metrics) {
      const metricsFile = `/tmp/skill-metrics-${Date.now()}.json`;
      const fs = await import('fs');
      fs.writeFileSync(metricsFile, JSON.stringify(metrics));
      result = await this.runOpenSpace(['skill-evolve', skillName, '--metrics', metricsFile]);
    } else {
      result = await this.runOpenSpace(['skill-evolve', skillName]);
    }
    
    try {
      return JSON.parse(result);
    } catch {
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
  async learnFromWorkflow(executionLog: string, options?: { autoInstall?: boolean }): Promise<LearnResult> {
    console.info(`[SkillEvolution] AUTO-LEARN: capturing from workflow`);
    
    const logFile = `/tmp/workflow-${Date.now()}.log`;
    const fs = await import('fs');
    fs.writeFileSync(logFile, executionLog);
    
    const result = await this.runOpenSpace([
      'skill-learn',
      '--from', logFile,
      '--auto-install', options?.autoInstall ? 'true' : 'false'
    ]);
    
    try {
      return JSON.parse(result);
    } catch {
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
  private async runOpenSpace(args: string[]): Promise<string> {
    // Build env with MiniMax/OpenAI-compatible LLM
    const env: Record<string, string> = {
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
      const proc = spawn('openspace', args, {
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
        } else {
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
  async isConfigured(): Promise<{ available: boolean; error?: string }> {
    try {
      await this.runOpenSpace(['--version']);
      return { available: true };
    } catch (err) {
      return { 
        available: false, 
        error: 'OpenSpace not installed. Run: pip install -e ~/OpenSpace' 
      };
    }
  }
}

/**
 * Create SkillEvolution instance from Argentum credentials
 * Reads API keys from ~/.openclaw/credentials/telegram.json
 */
export async function createSkillEvolution(): Promise<SkillEvolution> {
  const credsPath = join(homedir(), '.openclaw', 'credentials', 'telegram.json');
  let minimaxKey = '';
  
  if (existsSync(credsPath)) {
    try {
      const creds = JSON.parse(readFileSync(credsPath, 'utf-8'));
      minimaxKey = typeof creds.minimax === 'string' ? creds.minimax : '';
    } catch (err) {
      console.warn(
        `[SkillEvolution] Failed to read credentials from ${credsPath}: ${
          err instanceof Error ? err.message : String(err)
        }`,
      );
    }
  }
  
  return new SkillEvolution({
    llmProvider: 'minimax',
    apiKey: firstNonEmpty(minimaxKey, process.env.MINIMAX_API_KEY),
  });
}
