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

  test('&& operator returns true when both conditions are true', async () => {
    const handler = (mesh as any).stepHandlers.get('condition');
    const step = { config: { condition: 'stock > 0 && price < 50' }, nextSteps: ['a', 'b'] };
    const r = await handler(step, { price: 40, stock: 2 }, {} as any);
    expect(r.result).toBe(true);
    expect(r.nextStep).toBe('a'); // first nextStep when true
  });

  test('&& operator returns false when first condition is false', async () => {
    const handler = (mesh as any).stepHandlers.get('condition');
    const step = { config: { condition: 'stock > 0 && price < 50' }, nextSteps: ['a', 'b'] };
    const r = await handler(step, { price: 60, stock: 2 }, {} as any);
    expect(r.result).toBe(false);
    expect(r.nextStep).toBe('b'); // second nextStep when false
  });

  test('|| operator returns true when at least one condition is true', async () => {
    const handler = (mesh as any).stepHandlers.get('condition');
    const step = { config: { condition: 'stock > 10 || price < 50' }, nextSteps: ['a', 'b'] };
    const r = await handler(step, { price: 40, stock: 5 }, {} as any);
    expect(r.result).toBe(true);
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
