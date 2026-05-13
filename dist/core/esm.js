"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.importEsmModule = importEsmModule;
// Keep native dynamic import() in CommonJS output so packaged builds can load ESM-only dependencies.
// eslint-disable-next-line no-new-func
const nativeDynamicImport = new Function('specifier', 'return import(specifier)');
const allowedEsmSpecifiers = new Set([
    '@clack/prompts',
]);
function importEsmModule(specifier) {
    if (!allowedEsmSpecifiers.has(specifier)) {
        throw new Error(`Refusing to dynamically import unexpected ESM module: ${specifier}`);
    }
    return nativeDynamicImport(specifier);
}
