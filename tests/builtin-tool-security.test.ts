import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join, resolve } from 'path';

import { createBuiltinTools } from '../src/index';

describe('built-in tool security', () => {
  test('filesystem tools are scoped to the configured workspace root', async () => {
    const parent = mkdtempSync(join(tmpdir(), 'argentum-tools-'));
    const workspace = join(parent, 'workspace');
    const sibling = join(parent, 'workspace-secret');
    mkdirSync(workspace);
    mkdirSync(sibling);
    writeFileSync(join(workspace, 'note.txt'), 'inside workspace');
    writeFileSync(join(sibling, 'token.txt'), 'outside workspace');

    try {
      const tools = createBuiltinTools({ enableFilesystemTools: true, workspaceRoot: workspace });
      const readFile = tools.find((tool) => tool.name === 'read_file');
      const writeFile = tools.find((tool) => tool.name === 'write_file');

      expect(await readFile?.execute({ path: 'note.txt' })).toBe('inside workspace');
      expect(await readFile?.execute({ path: join(sibling, 'token.txt') })).toContain(
        'outside the configured workspace',
      );

      const writeResult = await writeFile?.execute({
        path: 'nested/output.txt',
        content: 'created inside workspace',
      });
      expect(writeResult).toContain(resolve(workspace, 'nested', 'output.txt'));
      expect(readFileSync(join(workspace, 'nested', 'output.txt'), 'utf8')).toBe(
        'created inside workspace',
      );
      expect(await writeFile?.execute({ path: join(sibling, 'owned.txt'), content: 'nope' })).toContain(
        'outside the configured workspace',
      );
    } finally {
      rmSync(parent, { recursive: true, force: true });
    }
  });

  test('filesystem tools use ARGENTUM_TOOL_ROOT and ignore legacy tool roots', async () => {
    const parent = mkdtempSync(join(tmpdir(), 'argentum-env-tools-'));
    const workspace = join(parent, 'workspace');
    const legacyWorkspace = join(parent, 'legacy');
    mkdirSync(workspace);
    mkdirSync(legacyWorkspace);
    writeFileSync(join(workspace, 'note.txt'), 'argentum root');
    writeFileSync(join(legacyWorkspace, 'note.txt'), 'legacy root');

    const originalArgentumRoot = process.env.ARGENTUM_TOOL_ROOT;
    const originalLegacyRoot = process.env.AGCLAW_TOOL_ROOT;
    process.env.ARGENTUM_TOOL_ROOT = workspace;
    process.env.AGCLAW_TOOL_ROOT = legacyWorkspace;

    try {
      const readFile = createBuiltinTools({ enableFilesystemTools: true }).find(
        (tool) => tool.name === 'read_file',
      );

      expect(await readFile?.execute({ path: 'note.txt' })).toBe('argentum root');
    } finally {
      if (originalArgentumRoot === undefined) delete process.env.ARGENTUM_TOOL_ROOT;
      else process.env.ARGENTUM_TOOL_ROOT = originalArgentumRoot;
      if (originalLegacyRoot === undefined) delete process.env.AGCLAW_TOOL_ROOT;
      else process.env.AGCLAW_TOOL_ROOT = originalLegacyRoot;
      rmSync(parent, { recursive: true, force: true });
    }
  });

  test('shell tool runs from the configured workspace when explicitly enabled', async () => {
    const workspace = mkdtempSync(join(tmpdir(), 'argentum-shell-'));

    try {
      const runCommand = createBuiltinTools({
        enableShellTool: true,
        workspaceRoot: workspace,
      }).find((tool) => tool.name === 'run_command');

      const output = await runCommand?.execute({
        command: 'node -e "process.stdout.write(process.cwd())"',
      });

      expect(resolve(output ?? '')).toBe(resolve(workspace));
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
