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

// Bundle wpcap.dll (Npcap SDK) so the Windows release can sniff packets
// without requiring the user to install Npcap. Copy into build/ so the
// Tauri bundler includes it in the app directory.
const buildDir = path.join(__dirname, '..', 'build');
const dllName = 'wpcap.dll';
const srcEnv = process.env.NPCAP_DLL_PATH;
let src = srcEnv;
if (!src) {
  // CI: Npcap SDK extracted to C:\npcap-sdk by the release workflow
  if (process.platform === 'win32') {
    src = 'C:\\npcap-sdk\\Lib\\x64\\wpcap.dll';
  }
}
if (src && fs.existsSync(src)) {
  fs.copyFileSync(src, path.join(buildDir, dllName));
  console.log(`Bundled ${dllName} from ${src}`);
} else if (process.platform === 'win32') {
  console.warn(
    `WARNING: ${dllName} not found — Windows builds will lack packet capture. ` +
    `Set NPCAP_DLL_PATH or extract the Npcap SDK to C:\\npcap-sdk.`
  );
}
