// Settings management for Albion Online Translator

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

export function saveSettings(settings) {
  try {
    localStorage.setItem('albion-translator-settings', JSON.stringify(settings));
  } catch (e) {
    console.error('Failed to save settings:', e);
  }
}

export function applySettings(settings) {
  const root = document.documentElement;
  
  // Apply opacity
  root.style.setProperty('--app-opacity', settings.opacity);
  
  // Apply font size
  root.style.setProperty('--app-font-size', `${settings.fontSize}px`);
}

export { defaultSettings };
