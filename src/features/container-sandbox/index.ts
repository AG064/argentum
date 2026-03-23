/**
 * AG-Claw Container Sandbox
 *
 * Runs shell commands inside isolated Docker containers.
 * Features:
 *   - Network isolation (--network none)
 *   - Read-only root filesystem
 *   - Limited tmpfs for /tmp
 *   - Memory + CPU limits
 *   - Timeout enforcement (kills container after N ms)
 *   - Returns stdout, stderr, exit code
 */

import { spawn } from 'child_process';
import { resolve } from 'path';

import { createLogger } from '../../core/logger';
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';

// ─── Types ────────────────────────────────────────────────────

export interface SandboxConfig {
  enabled: boolean;
  image: string;
  memoryLimit: string;
  cpuLimit: string;
  timeoutMs: number;
  workspacePath: string;
  networkAccess: boolean;
  readOnlyRoot: boolean;
  tmpfsSize: string;
}

export interface SandboxResult {
  success: boolean;
  stdout: string;
  stderr: string;
  exitCode: number;
  durationMs: number;
  timedOut: boolean;
}

const DEFAULT_CONFIG: SandboxConfig = {
  enabled: false,
  image: 'node:20-alpine',
  memoryLimit: '256m',
  cpuLimit: '1.0',
  timeoutMs: 30000,
  workspacePath: resolve(process.cwd(), 'data/workspace'),
  networkAccess: false,
  readOnlyRoot: true,
  tmpfsSize: '64m',
};

// ─── Core execution ───────────────────────────────────────────

/**
 * Execute a shell command inside an isolated Docker container.
 *
 * @param command - Shell command to run
 * @param options - Sandbox configuration overrides
 */
export function runInSandbox(
  command: string,
  options: Partial<SandboxConfig> = {},
): Promise<SandboxResult> {
  const config: SandboxConfig = { ...DEFAULT_CONFIG, ...options };
  const startTime = Date.now();

  return new Promise<SandboxResult>((resolvePromise) => {
    const args = buildDockerArgs(config, command);

    const log = createLogger().child({ feature: 'container-sandbox' });
    log.debug('Sandbox exec', { command, image: config.image });

    const proc = spawn('docker', args, {
      stdio: ['pipe', 'pipe', 'pipe'],
      timeout: config.timeoutMs + 5000, // slight buffer over container timeout
    });

    let stdout = '';
    let stderr = '';
    let timedOut = false;

    // Timeout enforcement
    const killTimer = setTimeout(() => {
      timedOut = true;
      // Kill the docker container by name
      const containerName = args[args.indexOf('--name') + 1];
      if (containerName) {
        spawn('docker', ['kill', containerName], { stdio: 'ignore' });
      }
      proc.kill('SIGKILL');
    }, config.timeoutMs);

    proc.stdout.on('data', (chunk: Buffer) => {
      stdout += chunk.toString();
    });

    proc.stderr.on('data', (chunk: Buffer) => {
      stderr += chunk.toString();
    });

    proc.on('close', (code) => {
      clearTimeout(killTimer);
      const durationMs = Date.now() - startTime;

      resolvePromise({
        success: code === 0 && !timedOut,
        stdout: stdout.trim(),
        stderr: stderr.trim(),
        exitCode: code ?? -1,
        durationMs,
        timedOut,
      });
    });

    proc.on('error', (err) => {
      clearTimeout(killTimer);
      resolvePromise({
        success: false,
        stdout: '',
        stderr: `Failed to start sandbox: ${err.message}`,
        exitCode: -1,
        durationMs: Date.now() - startTime,
        timedOut: false,
      });
    });

    // Close stdin (no interactive input)
    proc.stdin.end();
  });
}

/**
 * Build docker run arguments for sandboxed execution.
 */
function buildDockerArgs(config: SandboxConfig, command: string): string[] {
  const containerName = `agclaw-sandbox-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;

  const args: string[] = [
    'run',
    '--rm',
    '--name', containerName,
    '--memory', config.memoryLimit,
    '--cpus', config.cpuLimit,
    '--pids-limit', '64',
  ];

  // Network isolation
  if (!config.networkAccess) {
    args.push('--network', 'none');
  }

  // Read-only root filesystem
  if (config.readOnlyRoot) {
    args.push('--read-only');
  }

  // Tmpfs for /tmp
  args.push('--tmpfs', `/tmp:size=${config.tmpfsSize}`);

  // Mount workspace as read-only
  const ws = resolve(config.workspacePath);
  args.push('-v', `${ws}:/app/workspace:ro`);

  // Working directory
  args.push('-w', '/app/workspace');

  // Image
  args.push(config.image);

  // Validate and split command into argv (no shell interpretation)
  const dangerous = /[;|&`$\(\)\{\}]/;
  if (dangerous.test(command)) {
    throw new Error('Command contains forbidden characters');
  }

  // Whitelist common allowed base commands
  const whitelist = new Set(['ls','cat','echo','grep','find','node','python','npm','curl','wget','stat','du','df','ps','whoami','id']);

  // Simple split respecting quotes
  const parts: string[] = [];
  const re = /("[^"]*"|'[^']*'|\S+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(command)) !== null) {
    let p = m[0];
    if ((p.startsWith('"') && p.endsWith('"')) || (p.startsWith("'") && p.endsWith("'"))) {
      p = p.slice(1, -1);
    }
    parts.push(p);
  }
  if (parts.length === 0) parts.push('sh','-c','');

  // Check base command against whitelist
  const base = parts[0] as string;
  if (!whitelist.has(base)) {
    throw new Error('Command not allowed');
  }

  // Append command parts as argv (docker will run them directly, no shell)
  for (const p of parts) args.push(p);

  return args;
}

// ─── Feature module (for plugin-loader integration) ───────────

class ContainerSandboxFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'container-sandbox',
    version: '0.2.0',
    description: 'Docker-based command sandboxing with isolation',
    dependencies: [],
  };

  private config: SandboxConfig = { ...DEFAULT_CONFIG };
  private ctx!: FeatureContext;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      ...DEFAULT_CONFIG,
      ...(config as Partial<SandboxConfig>),
    };
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Container Sandbox ready', {
      image: this.config.image,
      memory: this.config.memoryLimit,
      timeout: `${this.config.timeoutMs}ms`,
      network: this.config.networkAccess ? 'enabled' : 'isolated',
    });
  }

  async stop(): Promise<void> {
    this.ctx.logger.info('Container Sandbox stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      // Quick check: can we reach docker?
      const result = await runInSandbox('echo ok', {
        ...this.config,
        timeoutMs: 5000,
      });

      return {
        healthy: result.success,
        message: result.success ? 'Docker sandbox operational' : result.stderr,
        details: { exitCode: result.exitCode },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /**
   * Execute a command in the sandbox (public API for other features).
   */
  async execute(command: string, overrides?: Partial<SandboxConfig>): Promise<SandboxResult> {
    return runInSandbox(command, { ...this.config, ...overrides });
  }
}

export default new ContainerSandboxFeature();
