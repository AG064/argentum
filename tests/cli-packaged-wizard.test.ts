import { readFileSync } from 'fs';

describe('packaged first-run wizard', () => {
  test('loads clack prompts through native dynamic import for packaged CommonJS builds', () => {
    const cliSource = readFileSync('src/cli.ts', 'utf8');

    expect(cliSource).toContain("importEsmModule<typeof ClackPrompts>('@clack/prompts')");
    expect(cliSource).not.toContain("await import('@clack/prompts')");
  });
});
