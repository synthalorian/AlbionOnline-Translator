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

// Bundle Npcap DLL(s) so the Windows release can sniff packets
// without requiring the user to install Npcap. Copy into build/ so the
// Tauri bundler includes them in the app directory.
const buildDir = path.join(__dirname, '..', 'build');
const SDK_ROOT = process.env.NPCAP_SDK_PATH || (process.platform === 'win32' ? 'C:\\npcap-sdk' : '');
const libDir = path.join(SDK_ROOT, 'Lib', 'x64');

// Npcap SDK ships either wpcap.dll or vpcap.dll depending on version.
// Prefer the one that actually exists in the SDK Lib\x64 folder.
function bundleDllIfPresent(name) {
  const src = path.join(libDir, name);
  if (fs.existsSync(src)) {
    fs.copyFileSync(src, path.join(buildDir, name));
    console.log(`Bundled ${name} from ${src}`);
    return true;
  }
  return false;
}

let bundled = false;
if (SDK_ROOT && fs.existsSync(libDir)) {
  bundled = bundleDllIfPresent('vpcap.dll') || bundleDllIfPresent('wpcap.dll');
}

if (!bundled && process.platform === 'win32') {
  console.warn(
    'WARNING: No Npcap DLL (vpcap.dll / wpcap.dll) found — Windows builds will lack packet capture. ' +
    'Set NPCAP_SDK_PATH or extract the Npcap SDK to C:\\npcap-sdk.'
  );
}
