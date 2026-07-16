import { execFileSync } from 'child_process';
import { existsSync, readFileSync, readdirSync } from 'fs';
import path from 'path';

import ts from 'typescript';

import { ARGENTUM_BUNDLED_SKILLS } from '../../src/ui/desktop/modules/skills-catalog';

const root = path.resolve(__dirname, '../..');
const desktopRoot = path.join(root, 'src/ui/desktop');

function javascriptFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return javascriptFiles(entryPath);
    return entry.isFile() && entry.name.endsWith('.js') ? [entryPath] : [];
  });
}

describe('desktop release assets', () => {
  it('keeps generated desktop modules synchronized with their sources', () => {
    expect(() =>
      execFileSync(process.execPath, ['scripts/sync-desktop-assets.js', '--check'], {
        cwd: root,
        stdio: 'pipe',
      }),
    ).not.toThrow();
  });

  it('resolves every relative static JavaScript module import', () => {
    const missing: string[] = [];
    for (const file of javascriptFiles(desktopRoot)) {
      const source = readFileSync(file, 'utf8');
      const imports = ts.preProcessFile(source).importedFiles;
      for (const imported of imports) {
        if (!imported.fileName.startsWith('.')) continue;
        const resolved = path.resolve(path.dirname(file), imported.fileName);
        if (!existsSync(resolved)) {
          missing.push(`${path.relative(root, file)} -> ${imported.fileName}`);
        }
      }
    }

    expect(missing).toEqual([]);
  });

  it('lists only skills that are actually bundled in the source tree', () => {
    for (const skill of ARGENTUM_BUNDLED_SKILLS) {
      expect(existsSync(path.join(root, 'src/features', skill.name, 'SKILL.md'))).toBe(true);
    }
  });
});
