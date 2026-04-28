"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.importEsmModule = importEsmModule;
// Keep native dynamic import() in CommonJS output so packaged builds can load ESM-only dependencies.
// eslint-disable-next-line no-new-func
const nativeDynamicImport = new Function('specifier', 'return import(specifier)');
function importEsmModule(specifier) {
    return nativeDynamicImport(specifier);
}
