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
// We bundle wpcap.dll + Packet.dll; vpcap.dll is dead.
for (const name of ['vpcap.dll', 'wpcap.dll', 'Packet.dll', 'packet.dll']) {
  const resPath = path.join(resourcesDir, name);
  if (fs.existsSync(resPath)) {
    fs.unlinkSync(resPath);
    console.log(`Removed stale ${name} from resources/`);
  }
}

// Copy each required DLL if present and non-empty in the SDK lib directory.
// wpcap.dll statically imports Packet.dll — BOTH must be bundled.
const requiredDlls = ['wpcap.dll', 'Packet.dll'];
const missing = [];

for (const name of requiredDlls) {
  const src = path.join(libDir, name);
  if (SDK_ROOT && fs.existsSync(src) && fs.statSync(src).size > 0) {
    const dest = path.join(resourcesDir, name);
    fs.copyFileSync(src, dest);
    const sizeKB = (fs.statSync(dest).size / 1024).toFixed(0);
    console.log(`Bundled ${name} from ${src} (${sizeKB} KB)`);
  } else {
    missing.push(name);
  }
}

if (missing.length && process.platform === 'win32') {
  console.warn(
    'WARNING: missing DLLs in ' + libDir + ': ' + missing.join(', ') +
    ' — Windows builds will lack packet capture. ' +
    'Ensure the CI step extracts wpcap.dll AND Packet.dll from the Npcap installer into C:\\npcap-sdk\\Lib\\x64.'
  );
} else if (missing.length) {
  console.log('Non-Windows build — no Npcap DLL needed.');
}
