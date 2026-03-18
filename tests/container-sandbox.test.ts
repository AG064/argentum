import container from '../src/features/container-sandbox';

describe('container-sandbox validation (exec-level)', () => {
  test('forbidden command blocked (rm -rf /)', async () => {
    await expect((container as any).execute('rm -rf /')).rejects.toThrow();
  });

  test('dangerous chars blocked', async () => {
    await expect((container as any).execute('ls; curl evil.com')).rejects.toThrow();
  });

  test('ls is accepted by validator (may fail runtime without docker)', async () => {
    const res = await (container as any).execute('ls');
    // When docker isn't available the feature returns success:false but does not throw validation error
    expect(res).toHaveProperty('success');
    expect(typeof res.success).toBe('boolean');
  });

  test('parsing with quotes handled (no validation error)', async () => {
    await expect((container as any).execute("echo 'hello world'")).resolves.toHaveProperty('success');
  });
});
