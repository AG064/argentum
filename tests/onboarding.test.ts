import { mkdtempSync, readFileSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { parse } from 'yaml';

import { ConfigSchema } from '../src/core/config';
import {
  createOnboardingProfile,
  generateWebchatAuthToken,
  writeOnboardingProfile,
} from '../src/core/onboarding';

describe('secure onboarding profile', () => {
  test('default onboarding profile is schema-valid and loopback-only', () => {
    const profile = createOnboardingProfile();
    const parsed = ConfigSchema.parse(profile.config);

    expect(parsed.server.host).toBe('127.0.0.1');
    expect(parsed.server.cors.origins).toEqual([
      'http://127.0.0.1:3000',
      'http://localhost:3000',
    ]);
    expect(parsed.security.allowlistMode).toBe('strict');
    expect(parsed.channels.telegram.enabled).toBe(false);
    expect(parsed.channels.webchat.enabled).toBe(false);
    expect(parsed.features.webchat.enabled).toBe(false);
    expect(profile.env).not.toHaveProperty('AGCLAW_WEBCHAT_AUTH_TOKEN');
  });

  test('webchat onboarding requires and stores a generated auth token', () => {
    const token = generateWebchatAuthToken();
    const profile = createOnboardingProfile({
      featureCategories: ['comm'],
      webchatAuthToken: token,
    });
    const parsed = ConfigSchema.parse(profile.config);

    expect(token).toHaveLength(64);
    expect(parsed.features.webchat.enabled).toBe(true);
    expect(parsed.features.webchat.host).toBe('127.0.0.1');
    expect(parsed.features.webchat.requireAuth).toBe(true);
    expect(parsed.features.webchat.authToken).toBeUndefined();
    expect(parsed.channels.webchat.enabled).toBe(true);
    expect(parsed.channels.webchat.authToken).toBeUndefined();
    expect(profile.env.AGCLAW_WEBCHAT_AUTH_TOKEN).toBe(token);
  });

  test('telegram is not enabled unless it has an allowlist or explicit allow-all', () => {
    const blocked = createOnboardingProfile({
      telegram: { token: '123:abc' },
    });
    const allowed = createOnboardingProfile({
      telegram: { token: '123:abc', allowedUsers: [42] },
    });

    expect(ConfigSchema.parse(blocked.config).channels.telegram.enabled).toBe(false);
    expect(blocked.env).not.toHaveProperty('AGCLAW_TELEGRAM_TOKEN');
    expect(blocked.warnings.join('\n')).toMatch(/Telegram/i);

    const parsedAllowed = ConfigSchema.parse(allowed.config);
    expect(parsedAllowed.channels.telegram.enabled).toBe(true);
    expect(parsedAllowed.channels.telegram.allowedUsers).toEqual([42]);
    expect(allowed.env.AGCLAW_TELEGRAM_TOKEN).toBe('123:abc');
  });

  test('writes parseable YAML config and dotenv files without secrets in YAML', () => {
    const dir = mkdtempSync(join(tmpdir(), 'agclaw-onboarding-'));
    try {
      const profile = createOnboardingProfile({
        provider: 'nvidia',
        apiKey: 'nv-secret',
        featureCategories: ['comm'],
        webchatAuthToken: 'a'.repeat(64),
      });

      const written = writeOnboardingProfile(dir, profile);
      const yaml = readFileSync(written.configPath, 'utf8');
      const env = readFileSync(written.envPath, 'utf8');
      const parsed = ConfigSchema.parse(parse(yaml));

      expect(parsed.llm.default).toBe('nvidia');
      expect(yaml).not.toContain('nv-secret');
      expect(yaml).not.toContain('a'.repeat(64));
      expect(env).toContain('NVIDIA_API_KEY=nv-secret');
      expect(env).toContain(`AGCLAW_WEBCHAT_AUTH_TOKEN=${'a'.repeat(64)}`);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
