#!/usr/bin/env node
/**
 * postinstall-patch.js
 * After npm install, patches eslint-plugin-import for minimatch compatibility
 * Run automatically via npm postinstall hook
 */
const fs = require('fs');
const path = require('path');

const ruleFile = path.join(__dirname, '..', 'node_modules', 'eslint-plugin-import', 'lib', 'rules', 'no-extraneous-dependencies.js');

try {
    let content = fs.readFileSync(ruleFile, 'utf8');
    
    if (content.includes('_minimatch_default_patch_applied')) {
        // Already patched
        console.log('[patch] eslint-plugin-import already patched');
        process.exit(0);
    }
    
    // Find the minimatch require line and add .default fallback for CJS minimatch@9
    const reqPattern = /var _minimatch = require\(['"]minimatch['"]\);var _minimatch2 = _interopRequireDefault\(_minimatch\);?/;
    
    if (!reqPattern.test(content)) {
        console.log('[patch] minimatch require pattern not found — may already be fixed');
        process.exit(0);
    }
    
    const replacement = `var _minimatch = require('minimatch');var _minimatch2 = _interopRequireDefault(_minimatch);// _minimatch_default_patch_applied
if(typeof _minimatch2.default === 'undefined'){_minimatch2.default = _minimatch2.minimatch||function(p,pat,opts){return new _minimatch2.Minimatch(pat,opts).match(p);};}`;
    
    const newContent = content.replace(reqPattern, replacement);
    
    if (newContent === content) {
        console.log('[patch] Could not apply patch');
        process.exit(1);
    }
    
    fs.writeFileSync(ruleFile, newContent);
    console.log('[patch] eslint-plugin-import: added .default fallback for minimatch CJS');
} catch (e) {
    console.log('[patch] Note:', e.message);
}
