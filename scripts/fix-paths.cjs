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

// Bundle Npcap DLL(s) into src-tauri/resources/ so the Tauri v2 bundler
// includes them in the Windows app package. The pcap crate loads them at
// runtime via LoadLibrary — they must sit next to the .exe.
const resourcesDir = path.join(__dirname, '..', 'src-tauri', 'resources');
const SDK_ROOT = process.env.NPCAP_SDK_PATH || (process.platform === 'win32' ? 'C:\\npcap-sdk' : '');
const libDir = path.join(SDK_ROOT, 'Lib', 'x64');

// Ensure resources directory exists
if (!fs.existsSync(resourcesDir)) {
  fs.mkdirSync(resourcesDir, { recursive: true });
}

// Npcap SDK ships either wpcap.dll or vpcap.dll depending on version.
// Prefer the one that actually exists in the SDK Lib\x64 folder.
function bundleDllIfPresent(name) {
  const src = path.join(libDir, name);
  if (fs.existsSync(src)) {
    fs.copyFileSync(src, path.join(resourcesDir, name));
    console.log(`Bundled ${name} from ${src}`);
    return true;
  }
  return false;
}

let bundled = false;
if (SDK_ROOT && fs.existsSync(libDir)) {
  bundled = bundleDllIfPresent('vpcap.dll') || bundleDllIfPresent('wpcap.dll');
}

// On non-Windows builds the DLLs won't exist — create placeholders so the
// Tauri bundler's resource glob doesn't fail. They stay empty and are never
// loaded at runtime (the pcap crate only calls LoadLibrary on Windows).
for (const name of ['vpcap.dll', 'wpcap.dll']) {
  const resPath = path.join(resourcesDir, name);
  if (!fs.existsSync(resPath)) {
    fs.writeFileSync(resPath, '');
    console.log(`Placeholder ${name} (no Npcap SDK — will be skipped on non-Windows)`);
  }
}

if (!bundled && process.platform === 'win32') {
  console.warn(
    'WARNING: No Npcap DLL (vpcap.dll / wpcap.dll) found — Windows builds will lack packet capture. ' +
    'Set NPCAP_SDK_PATH or extract the Npcap SDK to C:\\npcap-sdk.'
  );
}
