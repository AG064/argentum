import mesh from '../src/features/mesh-workflows';

describe('mesh-workflows evaluateCondition', () => {
  beforeAll(async () => {
    // init with a lightweight context containing a logger and emit
    await (mesh as any).init?.({}, { logger: console, emit: () => {} });
    await (mesh as any).start?.();
  });

  test('simple expressions', () => {
    const handler = (mesh as any).stepHandlers.get('condition');
    expect(handler).toBeDefined();

    const step = { config: { condition: 'price > 100' }, nextSteps: ['a', 'b'] };
    const res = handler(step, { price: 120, stock: 3 }, {} as any);
    return res.then((r: any) => {
      expect(r.result).toBe(true);
    });
  });

  test('logical expression with variables (unsupported operator -> fails safe)', async () => {
    const handler = (mesh as any).stepHandlers.get('condition');
    const step = { config: { condition: 'stock > 0 && price < 50' }, nextSteps: ['a', 'b'] };
    const r = await handler(step, { price: 40, stock: 2 }, {} as any);
    // current evaluator does not support && and should return result:false and warn
    expect(r.result).toBe(false);
  });

  afterAll(async () => {
    await (mesh as any).stop?.();
  });

  test('does not execute code injection', async () => {
    const handler = (mesh as any).stepHandlers.get('condition');
    const step = { config: { condition: '1; process.exit(1)' }, nextSteps: ['a', 'b'] };
    const r = await handler(step, {}, {} as any);
    // should fail safely and return result: false (start code returns {result:false} on catch)
    expect(r.result).toBe(false);
  });

  test('member expressions', async () => {
    const handler = (mesh as any).stepHandlers.get('condition');
    const step = { config: { condition: "user.role == 'admin'" }, nextSteps: ['a', 'b'] };
    const r = await handler(step, { user: { role: 'admin' } }, {} as any);
    expect(r.result).toBe(true);
  });
});
