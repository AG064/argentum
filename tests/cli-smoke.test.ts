import { execFileSync } from 'child_process';
import { readFileSync } from 'fs';
import { resolve } from 'path';

const CLI = process.env.ARGENTUM_CLI ?? resolve(__dirname, '../dist/cli.js');
const PACKAGE_VERSION = JSON.parse(readFileSync(resolve(__dirname, '../package.json'), 'utf8'))
  .version as string;

function run(args: string[], env?: Record<string, string>): string {
  // Validate CLI is an absolute path to prevent arbitrary code execution
  const resolvedCLI = CLI;
  if (!resolvedCLI.startsWith('/')) {
    throw new Error('CLI path must be absolute');
  }
  // Use execFileSync to avoid shell interpretation — args passed directly to node process
  const safeEnv: Record<string, string> = { ...process.env, ARGENTUM_NO_BANNER: '1' };
  if (env) {
    for (const [key, value] of Object.entries(env)) {
      safeEnv[key] = typeof value === 'string' ? value.replace(/[\n\r]/g, '') : String(value);
    }
  }
  return execFileSync('node', [resolvedCLI, ...args], {
    encoding: 'utf8',
    env: safeEnv,
  });
}

describe('CLI smoke tests', () => {
  test('--version returns version', () => {
    const output = run(['--version']);
    expect(output).toContain(PACKAGE_VERSION);
  });

  test('--help works', () => {
    const output = run(['--help']);
    expect(output).toContain('argentum');
    expect(output).toContain('Usage');
  });

  test('--version does not throw', () => {
    expect(() => run(['--version'])).not.toThrow();
  });
});
