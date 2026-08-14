import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'fs';
import os from 'os';
import path from 'path';

import { ConfigManager, ConfigSchema } from '../../src/core/config';
import { PluginLoader, resolveFeatureEntryPath, toModuleImportSpecifier } from '../../src/core/plugin-loader';

describe('ConfigManager', () => {
  it('merges default YAML config with argentum.json overrides', () => {
    const originalCwd = process.cwd();
    const tempDir = mkdtempSync(path.join(os.tmpdir(), 'argentum-config-'));

    try {
      mkdirSync(path.join(tempDir, 'config'), { recursive: true });
      writeFileSync(
        path.join(tempDir, 'config', 'default.yaml'),
        [
          'server:',
          '  port: 1111',
          'features:',
          '  webchat:',
          '    enabled: false',
        ].join('\n'),
      );
      writeFileSync(
        path.join(tempDir, 'argentum.json'),
        JSON.stringify(
          {
            server: { port: 2222 },
            features: { webchat: { enabled: true } },
          },
          null,
          2,
        ),
      );

      process.chdir(tempDir);

      const configManager = new ConfigManager();
      expect(configManager.get().server.port).toBe(2222);
      expect(configManager.isFeatureEnabled('webchat')).toBe(true);
    } finally {
      process.chdir(originalCwd);
      rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('ignores legacy agclaw.json overrides when argentum.json is absent', () => {
    const originalCwd = process.cwd();
    const tempDir = mkdtempSync(path.join(os.tmpdir(), 'argentum-config-'));

    try {
      mkdirSync(path.join(tempDir, 'config'), { recursive: true });
      writeFileSync(
        path.join(tempDir, 'config', 'default.yaml'),
        [
          'server:',
          '  port: 1111',
          'features:',
          '  webchat:',
          '    enabled: false',
        ].join('\n'),
      );
      writeFileSync(
        path.join(tempDir, 'agclaw.json'),
        JSON.stringify(
          {
            server: { port: 3333 },
            features: { webchat: { enabled: true } },
          },
          null,
          2,
        ),
      );

      process.chdir(tempDir);

      const configManager = new ConfigManager();
      expect(configManager.get().server.port).toBe(1111);
      expect(configManager.isFeatureEnabled('webchat')).toBe(false);
    } finally {
      process.chdir(originalCwd);
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});

describe('PluginLoader', () => {
  it('resolves the feature entry file from a feature directory', () => {
    const tempDir = mkdtempSync(path.join(os.tmpdir(), 'ag-claw-features-'));
    const featuresRoot = path.join(tempDir, 'features');
    const webchatDir = path.join(featuresRoot, 'webchat');

    try {
      mkdirSync(webchatDir, { recursive: true });
      const entryPath = path.join(webchatDir, 'index.js');
      writeFileSync(entryPath, 'export default {};');

      expect(resolveFeatureEntryPath(webchatDir)).toBe(entryPath);
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('converts filesystem paths into file URLs for native import()', () => {
    const modulePath = path.join(
      'C:\\',
      'ag064',
      'AGX',
      'ag-claw',
      'features',
      'webchat',
      'index.js',
    );

    const specifier = toModuleImportSpecifier(modulePath);

    expect(specifier).toContain('/ag064/AGX/ag-claw/features/webchat/index.js');
    expect(specifier.startsWith('file:')).toBe(true);
  });

  it('does not import disabled feature modules during discovery', async () => {
    const tempDir = mkdtempSync(path.join(os.tmpdir(), 'argentum-features-'));
    const featuresRoot = path.join(tempDir, 'features');
    const disabledDir = path.join(featuresRoot, 'native-heavy');

    try {
      mkdirSync(disabledDir, { recursive: true });
      writeFileSync(
        path.join(disabledDir, 'index.js'),
        "throw new Error('native optional dependency should not load while disabled');",
      );

      const config = ConfigSchema.parse({
        features: {
          'native-heavy': {
            enabled: false,
          },
        },
      });
      const loader = new PluginLoader(config, featuresRoot);

      await expect(loader.loadAll()).resolves.toBeUndefined();
      expect(loader.getFeatureState('native-heavy')).toBe('disabled');
      expect(loader.listFeatures()).toContainEqual({
        name: 'native-heavy',
        state: 'disabled',
        version: 'disabled',
      });
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});
