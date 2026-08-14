const fs = require('node:fs');
const path = require('node:path');

const projectRoot = path.resolve(__dirname, '..');

for (const target of ['dist', '.tsbuildinfo']) {
  fs.rmSync(path.join(projectRoot, target), { recursive: true, force: true });
}

