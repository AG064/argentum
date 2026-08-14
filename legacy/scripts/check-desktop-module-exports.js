const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..', 'src', 'ui', 'desktop');

function collectJavaScriptFiles(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return collectJavaScriptFiles(entryPath);
    return entry.isFile() && entry.name.endsWith('.js') ? [entryPath] : [];
  });
}

function exportedNames(source) {
  return new Set(
    [
      ...source.matchAll(
        /export\s+(?:(?:async)\s+)?(?:const|let|var|function|class)\s+([A-Za-z_$][\w$]*)/g,
      ),
    ].map((match) => match[1]),
  );
}

function importedNames(specifier) {
  const named = specifier.match(/\{([\s\S]*?)\}/);
  if (!named) return [];
  return named[1]
    .split(',')
    .map((part) => part.trim().split(/\s+as\s+/)[0])
    .filter(Boolean);
}

const files = collectJavaScriptFiles(root);
const sources = new Map(files.map((file) => [file, fs.readFileSync(file, 'utf8')]));
const errors = [];

for (const [file, source] of sources) {
  for (const match of source.matchAll(/import\s+([\s\S]*?)\s+from\s+['"](\.[^'"]+)['"];?/g)) {
    const target = path.resolve(path.dirname(file), match[2]);
    const targetSource = sources.get(target);
    if (!targetSource) continue;

    const exports = exportedNames(targetSource);
    for (const name of importedNames(match[1])) {
      if (!exports.has(name)) {
        errors.push(
          `${path.relative(process.cwd(), file)} imports ${name} from ${path.relative(process.cwd(), target)}`,
        );
      }
    }
  }
}

if (errors.length > 0) {
  console.error('Desktop modules import names that are not exported:');
  for (const error of errors) console.error(`- ${error}`);
  process.exitCode = 1;
} else {
  console.log(`Desktop module export check passed for ${files.length} JavaScript files.`);
}
