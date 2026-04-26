import { readFileSync } from 'fs';

import { parse } from 'yaml';

import { ConfigSchema } from '../src/core/config';
import { createBuiltinTools } from '../src/index';

describe('secure defaults', () => {
  test('schema defaults do not expose public chat channels', () => {
    const config = ConfigSchema.parse({});

    expect(config.channels.telegram.enabled).toBe(false);
    expect(config.channels.webchat.enabled).toBe(false);
    expect(config.features.webchat.enabled).toBe(false);
    expect(config.security.allowlistMode).toBe('strict');
  });

  test('checked-in default config keeps public chat channels disabled', () => {
    const yaml = readFileSync('config/default.yaml', 'utf8');
    const config = ConfigSchema.parse(parse(yaml));

    expect(config.channels.telegram.enabled).toBe(false);
    expect(config.channels.webchat.enabled).toBe(false);
    expect(config.features.webchat.enabled).toBe(false);
    expect(config.security.allowlistMode).toBe('strict');
  });

  test('dangerous file and shell tools are not registered by default', () => {
    const names = createBuiltinTools().map((tool) => tool.name);

    expect(names).not.toEqual(expect.arrayContaining(['read_file', 'write_file', 'run_command']));
  });
});
