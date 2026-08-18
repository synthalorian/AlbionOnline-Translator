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

// Bundle Npcap runtime DLL into src-tauri/resources/ so the Tauri v2
// bundler includes it in the Windows app package. The pcap crate loads
// wpcap.dll at runtime via LoadLibrary — it must sit next to the .exe.
const resourcesDir = path.join(__dirname, '..', 'src-tauri', 'resources');
const SDK_ROOT = process.env.NPCAP_SDK_PATH || (process.platform === 'win32' ? 'C:\\npcap-sdk' : '');
const libDir = path.join(SDK_ROOT, 'Lib', 'x64');

// Ensure resources directory exists
if (!fs.existsSync(resourcesDir)) {
  fs.mkdirSync(resourcesDir, { recursive: true });
}

// Npcap SDK Lib\x64 contains wpcap.lib (import library) + optionally
// wpcap.dll (extracted from the Npcap installer by CI). Look for wpcap.dll
// and bundle whichever one lands there. vpcap.dll was the old name;
// modern Npcap (1.70+) uses wpcap.dll only — skip vpcap.dll.
function bundleDllIfPresent(name) {
  const src = path.join(libDir, name);
  if (fs.existsSync(src) && fs.statSync(src).size > 0) {
    fs.copyFileSync(src, path.join(resourcesDir, name));
    console.log(`Bundled ${name} from ${src} (${(fs.statSync(src).size / 1024).toFixed(0)} KB)`);
    return true;
  }
  return false;
}

let bundled = false;

// Prefer wpcap.dll (modern Npcap); fall back to vpcap.dll for legacy SDKs
bundled = bundleDllIfPresent('wpcap.dll');
if (!bundled) {
  bundled = bundleDllIfPresent('vpcap.dll');
}

// On non-Windows builds the DLL won't exist — remove any stale placeholder
// from a previous run so the Tauri bundler doesn't try to package an empty file.
const targetName = bundled ? (bundled === 'wpcap.dll' ? 'wpcap.dll' : 'vpcap.dll') : null;
for (const name of ['vpcap.dll', 'wpcap.dll']) {
  const resPath = path.join(resourcesDir, name);
  if (fs.existsSync(resPath)) {
    if (name === targetName) {
      // keep the one we just bundled
      continue;
    }
    // remove placeholder or wrong DLL
    fs.unlinkSync(resPath);
    console.log(`Removed stale ${name} from resources/`);
  }
}

if (!bundled && process.platform === 'win32') {
  console.warn(
    'WARNING: wpcap.dll not found in ' + libDir + ' — Windows builds will lack packet capture. ' +
    'Ensure the CI step extracts the Npcap installer DLL into C:\\npcap-sdk\\Lib\\x64.'
  );
}
