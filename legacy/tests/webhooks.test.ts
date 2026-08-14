import webhooks from '../src/features/webhooks';

describe('webhooks URL validation and blocking', () => {
  const instance: any = webhooks;

  test('https://example.com passes', () => {
    const ok = instance.validateUrl('https://example.com');
    expect(ok).toBe(true);
  });

  test('localhost blocked', () => {
    const ok = instance.validateUrl('http://localhost:8080');
    expect(ok).toBe(false);
  });

  test('private IP blocked', () => {
    expect(instance.validateUrl('http://10.0.0.1')).toBe(false);
    expect(instance.validateUrl('http://169.254.169.254')).toBe(false);
  });

  test('outbound webhooks require https', () => {
    expect(instance.validateUrl('http://example.com')).toBe(false);
  });

  test('credentials in webhook URLs are blocked', () => {
    expect(instance.validateUrl('https://user:pass@example.com')).toBe(false);
  });

  test('private IPv6 targets are blocked', () => {
    expect(instance.validateUrl('https://[::1]/')).toBe(false);
    expect(instance.validateUrl('https://[fc00::1]/')).toBe(false);
    expect(instance.validateUrl('https://[fe80::1]/')).toBe(false);
  });

  test('signature verification', () => {
    const payload = JSON.stringify({ foo: 'bar' });
    const secret = 's3cr3t';
    const sig = require('crypto').createHmac('sha256', secret).update(payload).digest('hex');
    expect(instance.verifySignature(payload, `sha256=${sig}`, secret)).toBe(true);
    expect(instance.verifySignature(payload, `sha256=bad`, secret)).toBe(false);
  });
});
