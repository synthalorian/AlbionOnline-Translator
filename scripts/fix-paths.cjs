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
//
// Modern Npcap (1.70+) ships only wpcap.dll — vpcap.dll is obsolete.
const resourcesDir = path.join(__dirname, '..', 'src-tauri', 'resources');
const SDK_ROOT = process.env.NPCAP_SDK_PATH || (process.platform === 'win32' ? 'C:\\npcap-sdk' : '');
const libDir = path.join(SDK_ROOT, 'Lib', 'x64');

// Ensure resources directory exists
if (!fs.existsSync(resourcesDir)) {
  fs.mkdirSync(resourcesDir, { recursive: true });
}

// Clean up any stale DLLs from previous runs or older configs.
// We only ever bundle wpcap.dll; vpcap.dll is dead.
for (const name of ['vpcap.dll', 'wpcap.dll']) {
  const resPath = path.join(resourcesDir, name);
  if (fs.existsSync(resPath)) {
    fs.unlinkSync(resPath);
    console.log(`Removed stale ${name} from resources/`);
  }
}

// Copy wpcap.dll if present and non-empty in the SDK lib directory
const wpcapSrc = path.join(libDir, 'wpcap.dll');
let bundled = false;

if (SDK_ROOT && fs.existsSync(wpcapSrc) && fs.statSync(wpcapSrc).size > 0) {
  const dest = path.join(resourcesDir, 'wpcap.dll');
  fs.copyFileSync(wpcapSrc, dest);
  const sizeKB = (fs.statSync(dest).size / 1024).toFixed(0);
  console.log(`Bundled wpcap.dll from ${wpcapSrc} (${sizeKB} KB)`);
  bundled = true;
}

if (!bundled && process.platform === 'win32') {
  console.warn(
    'WARNING: wpcap.dll not found in ' + libDir + ' — Windows builds will lack packet capture. ' +
    'Ensure the CI step extracts the Npcap installer DLL into C:\\npcap-sdk\\Lib\\x64.'
  );
} else if (!bundled) {
  console.log('Non-Windows build — no Npcap DLL needed.');
}
