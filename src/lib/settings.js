// Settings management for Albion Online Translator

/** @typedef {{ opacity: number, fontSize: number, maxMessages: number, showTimestamps: boolean, showOriginal: boolean, showTranslated: boolean, clickThrough: boolean, alwaysOnTop: boolean, theme: string, targetLanguage: string }} Settings */

const defaultSettings = {
  opacity: 0.92,
  fontSize: 13,
  maxMessages: 100,
  showTimestamps: true,
  showOriginal: true,
  showTranslated: true,
  clickThrough: false,
  alwaysOnTop: true,
  theme: 'synthwave-84',
  targetLanguage: 'en',
  // Per-channel visibility toggles — every Albion chat channel, all on by default.
  // Language channels (English, Español, etc.) are dropped at decode time.
  channelFilters: {
    Say: true,
    Whisper: true,
    Party: true,
    Guild: true,
    Alliance: true,
    Global: true,
    Trade: true,
    LFG: true,
    Recruitment: true,
    Faction: true,
    Unknown: true,
  },
};

export function loadSettings() {
  try {
    const stored = localStorage.getItem('albion-translator-settings');
    if (stored) {
      return { ...defaultSettings, ...JSON.parse(stored) };
    }
  } catch (e) {
    console.error('Failed to load settings:', e);
  }
  return { ...defaultSettings };
}

/** @param {Settings} settings */
export function saveSettings(settings) {
  try {
    localStorage.setItem('albion-translator-settings', JSON.stringify(settings));
  } catch (e) {
    console.error('Failed to save settings:', e);
  }
}

/** @param {Settings} settings */
export function applySettings(settings) {
  const root = document.documentElement;
  
  // Apply opacity
  root.style.setProperty('--app-opacity', String(settings.opacity));
  
  // Apply font size
  root.style.setProperty('--app-font-size', `${settings.fontSize}px`);
}

export { defaultSettings };
