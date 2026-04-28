// Keep native dynamic import() in CommonJS output so packaged builds can load ESM-only dependencies.
// eslint-disable-next-line no-new-func
const nativeDynamicImport = new Function(
  'specifier',
  'return import(specifier)',
) as <T>(specifier: string) => Promise<T>;

export function importEsmModule<T>(specifier: string): Promise<T> {
  return nativeDynamicImport<T>(specifier);
}
