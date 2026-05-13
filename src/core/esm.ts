// Keep native dynamic import() in CommonJS output so packaged builds can load ESM-only dependencies.
// eslint-disable-next-line no-new-func
const nativeDynamicImport = new Function(
  'specifier',
  'return import(specifier)',
) as <T>(specifier: string) => Promise<T>;

const allowedEsmSpecifiers = new Set<string>([
  '@clack/prompts',
]);

export function importEsmModule<T>(specifier: string): Promise<T> {
  if (!allowedEsmSpecifiers.has(specifier)) {
    throw new Error(`Refusing to dynamically import unexpected ESM module: ${specifier}`);
  }
  return nativeDynamicImport<T>(specifier);
}
