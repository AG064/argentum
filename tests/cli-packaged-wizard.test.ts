import { readFileSync } from 'fs';

describe('packaged first-run wizard', () => {
  test('uses packaged-safe onboarding before loading clack prompts', () => {
    const cliSource = readFileSync('src/cli.ts', 'utf8');

    const packagedFallbackIndex = cliSource.indexOf('if (isPackagedRuntime())');
    const clackImportIndex = cliSource.indexOf("importEsmModule<typeof ClackPrompts>('@clack/prompts')");

    expect(cliSource).toContain('async function cmdOnboardBasic');
    expect(packagedFallbackIndex).toBeGreaterThanOrEqual(0);
    expect(clackImportIndex).toBeGreaterThanOrEqual(0);
    expect(packagedFallbackIndex).toBeLessThan(clackImportIndex);
    expect(cliSource).toContain("importEsmModule<typeof ClackPrompts>('@clack/prompts')");
    expect(cliSource).not.toContain("await import('@clack/prompts')");
  });

  test('sets the user-facing process and console title to Argentum', () => {
    const cliSource = readFileSync('src/cli.ts', 'utf8');

    expect(cliSource).toContain("const PROGRAM_TITLE = 'Argentum'");
    expect(cliSource).toContain('setProgramTitle(PROGRAM_TITLE)');
    expect(cliSource).toContain('process.title = title');
  });
});
