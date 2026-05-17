import { execSync } from 'child_process';
import { resolve } from 'path';

const CLI = process.env.ARGENTUM_CLI ?? resolve(__dirname, '../dist/cli.js');

function run(args: string[], env?: Record<string, string>): string {
  return execSync(`node ${CLI} ${args.join(' ')}`, {
    encoding: 'utf8',
    env: { ...process.env, ...env, ARGENTUM_NO_BANNER: '1' },
  });
}

describe('CLI smoke tests', () => {
  test('--version returns version', () => {
    const output = run(['--version']);
    expect(output).toMatch(/0\.0\.6/);
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
