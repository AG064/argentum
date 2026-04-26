import apiGateway from '../src/features/api-gateway';
import rateLimiting from '../src/features/rate-limiting';

describe('API gateway security', () => {
  const instance = apiGateway as any;

  beforeEach(() => {
    instance.ctx = {
      logger: {
        debug: jest.fn(),
        info: jest.fn(),
        warn: jest.fn(),
        error: jest.fn(),
      },
    };
    instance.config = {
      enabled: true,
      port: 0,
      path: '/api',
      apiKeys: ['ak_test'],
      rateLimit: { windowMs: 60000, max: 100, byApiKey: true },
    };
    instance.endpoints = new Map();
    instance.endpoints.set('DELETE', [
      {
        path: '/api/tokens/:key',
        method: 'DELETE',
        handler: jest.fn(),
        requiresAuth: true,
        rateLimited: true,
      },
    ]);
  });

  test('matches registered parameterized routes before auth decisions', () => {
    const endpoint = instance.findEndpoint('DELETE', '/api/tokens/ak_test');

    expect(endpoint?.path).toBe('/api/tokens/:key');
    expect(endpoint?.requiresAuth).toBe(true);
  });

  test('does not match unrelated api paths as public endpoints', () => {
    expect(instance.findEndpoint('DELETE', '/api/tokens')).toBeUndefined();
    expect(instance.findEndpoint('GET', '/api/tokens/ak_test')).toBeUndefined();
  });

  test('stores API tokens as hashes and only returns the raw key once', () => {
    instance.apiTokens = new Map();

    const created = instance.createToken('ci-token', 1, ['tokens:read']);
    const stored = Array.from(instance.apiTokens.values())[0];

    expect(created.key).toMatch(/^ak_/);
    expect(instance.apiTokens.has(created.key)).toBe(false);
    expect(stored.keyHash).toBeTruthy();
    expect(stored.keyHash).not.toContain(created.key);
    expect(stored.keyPreview).toMatch(/^ak_/);
    expect(instance.authenticateApiKey(created.key)?.name).toBe('ci-token');
    expect(instance.listTokens()[0]).not.toHaveProperty('key');
  });

  test('rejects expired tokens and enforces endpoint scopes', () => {
    instance.apiTokens = new Map();
    const expired = instance.createToken('expired', -1, ['tokens:read']);
    const scoped = instance.createToken('reader', 1, ['tokens:read']);

    expect(instance.authenticateApiKey(expired.key)).toBeNull();
    const token = instance.authenticateApiKey(scoped.key);

    expect(instance.tokenHasScope(token, 'tokens:read')).toBe(true);
    expect(instance.tokenHasScope(token, 'tokens:write')).toBe(false);
  });

  test('rate limits repeated unauthorized requests on protected endpoints', () => {
    (rateLimiting as any).initDb();
    const originalRateLimit = instance.config.rateLimit;
    instance.config.rateLimit = { windowMs: 60000, max: 1, byApiKey: false };
    rateLimiting.reset('127.0.0.1');

    const req: any = {
      method: 'DELETE',
      path: '/api/tokens/ak_test',
      headers: {},
      ip: '127.0.0.1',
    };
    const res: any = {
      status: jest.fn().mockReturnThis(),
      json: jest.fn().mockReturnThis(),
    };
    const next = jest.fn();

    instance.authMiddleware(req, res, next);
    expect(res.status).toHaveBeenCalledWith(401);
    expect(next).not.toHaveBeenCalled();

    res.status.mockClear();
    res.json.mockClear();
    next.mockClear();

    instance.authMiddleware(req, res, next);
    expect(res.status).toHaveBeenCalledWith(429);
    expect(next).not.toHaveBeenCalled();

    rateLimiting.reset('127.0.0.1');
    instance.config.rateLimit = originalRateLimit;
  });
});
