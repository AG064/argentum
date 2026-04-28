const nativeDynamicImport = new Function(
  'specifier',
  'return import(specifier)',
) as <T>(specifier: string) => Promise<T>;

export function importEsmModule<T>(specifier: string): Promise<T> {
  return nativeDynamicImport<T>(specifier);
}
