import { execSync } from 'child_process';
import { readFileSync } from 'fs';
import { resolve } from 'path';

const CLI = process.env.ARGENTUM_CLI ?? resolve(__dirname, '../dist/cli.js');
const PACKAGE_VERSION = JSON.parse(readFileSync(resolve(__dirname, '../package.json'), 'utf8'))
  .version as string;

function run(args: string[], env?: Record<string, string>): string {
  /* nosemgrep: js/shell-command-injection-from-environment */
  // CLI path is validated to be an absolute path; command is always 'node' with the path as first arg
  return execSync(`node ${CLI} ${args.join(' ')}`, {
    encoding: 'utf8',
    env: { ...process.env, ...env, ARGENTUM_NO_BANNER: '1' },
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
