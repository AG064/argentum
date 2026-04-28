import { ARGENTUM_BANNER } from '../src/core/branding';

describe('CLI branding', () => {
  test('banner spells Argentum in the block art', () => {
    expect(ARGENTUM_BANNER).toContain('ARGENTUM');
    expect(ARGENTUM_BANNER).not.toContain('GIXEUNKS');
  });
});
