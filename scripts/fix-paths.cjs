const fs = require('fs');
const path = require('path');

const indexPath = path.join(__dirname, '..', 'build', 'index.html');
const html = fs.readFileSync(indexPath, 'utf8');

// adapter-static emits absolute paths (/favicon.png, /_app/immutable/...)
// These 404 when loaded from file:// in a Tauri release.
// Rewrite to relative (./) so assets resolve from the file system.
const fixed = html
  .replaceAll('="/_app/', '="./_app/')
  .replaceAll('="/favicon.png"', '="./favicon.png"')
  .replaceAll('="/svelte.svg"', '="./svelte.svg"')
  .replaceAll('="/tauri.svg"', '="./tauri.svg"')
  .replaceAll('="/vite.svg"', '="./vite.svg"');

if (fixed !== html) {
  fs.writeFileSync(indexPath, fixed, 'utf8');
  console.log('Fixed asset paths in build/index.html');
} else {
  console.log('No path fixes needed');
}
