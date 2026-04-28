"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.importEsmModule = importEsmModule;
const nativeDynamicImport = new Function('specifier', 'return import(specifier)');
function importEsmModule(specifier) {
    return nativeDynamicImport(specifier);
}
