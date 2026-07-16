import { mkdtempSync, rmSync, writeFileSync } from 'fs';
import os from 'os';
import path from 'path';

import {
  defaultRouterConfig,
  loadRouterConfig,
  RouterAgent,
  type MessageContext,
} from '../../src/agents/router';

function context(message: string): MessageContext {
  return {
    sender: { id: 'telegram:42' },
    chat: { id: 'telegram:100', type: 'direct' },
    message,
    platform: 'telegram',
    timestamp: Date.now(),
  };
}

describe('RouterAgent', () => {
  it('loads the nested JSON format shipped in config/router.example.json', () => {
    const config = loadRouterConfig(path.resolve('config/router.example.json'));
    expect(config.defaultAgent).toBe('agx');
    expect(config.rules.length).toBeGreaterThan(0);
  });

  it('loads top-level YAML router configuration', () => {
    const tempDir = mkdtempSync(path.join(os.tmpdir(), 'argentum-router-'));
    const configPath = path.join(tempDir, 'router.yaml');
    try {
      writeFileSync(
        configPath,
        [
          'defaultAgent: main',
          'rules:',
          '  - condition: always',
          '    value: ""',
          '    targetAgent: fallback',
        ].join('\n'),
      );
      const config = loadRouterConfig(configPath);
      expect(config.defaultAgent).toBe('main');
      expect(config.rules[0].targetAgent).toBe('fallback');
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('matches string keywords case-insensitively', async () => {
    const router = new RouterAgent({
      defaultAgent: 'main',
      rules: [{ condition: 'keyword', value: 'ADMIN', targetAgent: 'admin' }],
    });

    await expect(router.route(context('please open the admin view'))).resolves.toMatchObject({
      agent: 'admin',
    });
  });

  it('rejects malformed rules instead of loading a partial configuration', () => {
    const tempDir = mkdtempSync(path.join(os.tmpdir(), 'argentum-router-'));
    const configPath = path.join(tempDir, 'router.json');
    const errorSpy = jest.spyOn(console, 'error').mockImplementation(() => undefined);
    const warningSpy = jest.spyOn(console, 'warn').mockImplementation(() => undefined);
    try {
      writeFileSync(
        configPath,
        JSON.stringify({ defaultAgent: 'main', rules: [{ condition: 'keyword', value: [] }] }),
      );
      expect(loadRouterConfig(configPath)).toEqual(defaultRouterConfig);
    } finally {
      errorSpy.mockRestore();
      warningSpy.mockRestore();
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});
