// Theme definitions for Albion Online Translator
// Each theme is a set of CSS custom properties

export const themes = {
  // === SYNTHWAVE COLLECTION ===
  'synthwave-84': {
    name: "Synthwave '84",
    emoji: '🌆',
    description: 'The OG. Deep purple-black, hot pink, cyan. Pure 1984.',
    category: 'synthwave',
    colors: {
      '--bg-primary': 'rgba(15, 10, 30, 0.95)',
      '--bg-secondary': 'rgba(25, 15, 45, 0.85)',
      '--bg-tertiary': 'rgba(35, 20, 60, 0.75)',
      '--bg-message': 'rgba(40, 25, 70, 0.6)',
      '--border-color': 'rgba(255, 100, 200, 0.25)',
      '--border-glow': 'rgba(255, 100, 200, 0.4)',
      '--text-primary': '#f0e0ff',
      '--text-secondary': '#c0a0e0',
      '--text-muted': '#8060a0',
      '--accent-primary': '#ff64c8',
      '--accent-secondary': '#64c8ff',
      '--accent-glow': '#ff64c8',
      '--status-online': '#64ff96',
      '--status-offline': '#9060c0',
      '--scrollbar-thumb': 'rgba(255, 100, 200, 0.35)',
      '--scrollbar-track': 'rgba(30, 15, 50, 0.4)',
    }
  },
  'neon-nights': {
    name: 'Neon Nights',
    emoji: '🌃',
    description: 'Tokyo midnight. Electric cyan + magenta on dark asphalt.',
    category: 'synthwave',
    colors: {
      '--bg-primary': 'rgba(10, 15, 25, 0.95)',
      '--bg-secondary': 'rgba(15, 25, 40, 0.85)',
      '--bg-tertiary': 'rgba(20, 35, 55, 0.75)',
      '--bg-message': 'rgba(25, 40, 65, 0.6)',
      '--border-color': 'rgba(0, 240, 255, 0.25)',
      '--border-glow': 'rgba(0, 240, 255, 0.4)',
      '--text-primary': '#e0f8ff',
      '--text-secondary': '#a0d8e8',
      '--text-muted': '#6090a8',
      '--accent-primary': '#00f0ff',
      '--accent-secondary': '#ff00a0',
      '--accent-glow': '#00f0ff',
      '--status-online': '#00ff88',
      '--status-offline': '#508090',
      '--scrollbar-thumb': 'rgba(0, 240, 255, 0.35)',
      '--scrollbar-track': 'rgba(15, 25, 40, 0.4)',
    }
  },
  'outrun': {
    name: 'Outrun',
    emoji: '🔥',
    description: 'Sunset strip. Orange/red gradients, Testarossa energy.',
    category: 'synthwave',
    colors: {
      '--bg-primary': 'rgba(25, 10, 15, 0.95)',
      '--bg-secondary': 'rgba(40, 18, 25, 0.85)',
      '--bg-tertiary': 'rgba(55, 25, 35, 0.75)',
      '--bg-message': 'rgba(65, 30, 40, 0.6)',
      '--border-color': 'rgba(255, 120, 60, 0.25)',
      '--border-glow': 'rgba(255, 120, 60, 0.4)',
      '--text-primary': '#ffe8d8',
      '--text-secondary': '#e0b8a0',
      '--text-muted': '#a07860',
      '--accent-primary': '#ff783c',
      '--accent-secondary': '#ffc832',
      '--accent-glow': '#ff783c',
      '--status-online': '#78ff64',
      '--status-offline': '#a06850',
      '--scrollbar-thumb': 'rgba(255, 120, 60, 0.35)',
      '--scrollbar-track': 'rgba(40, 18, 25, 0.4)',
    }
  },
  'vaporwave': {
    name: 'Vaporwave',
    emoji: '💜',
    description: 'A E S T H E T I C. Pastel pink + teal, mall-soft vibes.',
    category: 'synthwave',
    colors: {
      '--bg-primary': 'rgba(30, 20, 40, 0.95)',
      '--bg-secondary': 'rgba(45, 30, 55, 0.85)',
      '--bg-tertiary': 'rgba(60, 40, 70, 0.75)',
      '--bg-message': 'rgba(70, 50, 85, 0.6)',
      '--border-color': 'rgba(255, 150, 220, 0.25)',
      '--border-glow': 'rgba(255, 150, 220, 0.4)',
      '--text-primary': '#ffe8f8',
      '--text-secondary': '#e0c0d8',
      '--text-muted': '#a08098',
      '--accent-primary': '#ff96dc',
      '--accent-secondary': '#64ffd8',
      '--accent-glow': '#ff96dc',
      '--status-online': '#96ff96',
      '--status-offline': '#9878a0',
      '--scrollbar-thumb': 'rgba(255, 150, 220, 0.35)',
      '--scrollbar-track': 'rgba(45, 30, 55, 0.4)',
    }
  },

  // === ALBION COLLECTION ===
  'caerleon': {
    name: 'Caerleon',
    emoji: '⚔️',
    description: 'Outlaw City. Crimson + iron black. Danger everywhere.',
    category: 'albion',
    colors: {
      '--bg-primary': 'rgba(20, 12, 15, 0.95)',
      '--bg-secondary': 'rgba(32, 20, 25, 0.85)',
      '--bg-tertiary': 'rgba(45, 28, 35, 0.75)',
      '--bg-message': 'rgba(55, 35, 42, 0.6)',
      '--border-color': 'rgba(220, 50, 70, 0.25)',
      '--border-glow': 'rgba(220, 50, 70, 0.4)',
      '--text-primary': '#ffe0e0',
      '--text-secondary': '#d8a8a8',
      '--text-muted': '#987070',
      '--accent-primary': '#dc3246',
      '--accent-secondary': '#8a94a6',
      '--accent-glow': '#dc3246',
      '--status-online': '#50c878',
      '--status-offline': '#885858',
      '--scrollbar-thumb': 'rgba(220, 50, 70, 0.35)',
      '--scrollbar-track': 'rgba(32, 20, 25, 0.4)',
    }
  },
  'lymhurst': {
    name: 'Lymhurst',
    emoji: '🌿',
    description: 'Forest city. Lime + moss green. Nature thrives.',
    category: 'albion',
    colors: {
      '--bg-primary': 'rgba(12, 20, 12, 0.95)',
      '--bg-secondary': 'rgba(20, 32, 20, 0.85)',
      '--bg-tertiary': 'rgba(28, 45, 28, 0.75)',
      '--bg-message': 'rgba(35, 55, 35, 0.6)',
      '--border-color': 'rgba(120, 200, 80, 0.25)',
      '--border-glow': 'rgba(120, 200, 80, 0.4)',
      '--text-primary': '#e8ffe0',
      '--text-secondary': '#b0d8a0',
      '--text-muted': '#708860',
      '--accent-primary': '#78c850',
      '--accent-secondary': '#a0d890',
      '--accent-glow': '#78c850',
      '--status-online': '#50ff50',
      '--status-offline': '#607850',
      '--scrollbar-thumb': 'rgba(120, 200, 80, 0.35)',
      '--scrollbar-track': 'rgba(20, 32, 20, 0.4)',
    }
  },
  'fort-sterling': {
    name: 'Fort Sterling',
    emoji: '🏔️',
    description: 'Mountain fortress. Ice blue + steel. Cold and unyielding.',
    category: 'albion',
    colors: {
      '--bg-primary': 'rgba(12, 18, 25, 0.95)',
      '--bg-secondary': 'rgba(20, 30, 40, 0.85)',
      '--bg-tertiary': 'rgba(28, 42, 55, 0.75)',
      '--bg-message': 'rgba(35, 52, 68, 0.6)',
      '--border-color': 'rgba(100, 180, 255, 0.25)',
      '--border-glow': 'rgba(100, 180, 255, 0.4)',
      '--text-primary': '#e0f0ff',
      '--text-secondary': '#a8c8e0',
      '--text-muted': '#6888a0',
      '--accent-primary': '#64b4ff',
      '--accent-secondary': '#a0c8e8',
      '--accent-glow': '#64b4ff',
      '--status-online': '#50c8ff',
      '--status-offline': '#587088',
      '--scrollbar-thumb': 'rgba(100, 180, 255, 0.35)',
      '--scrollbar-track': 'rgba(20, 30, 40, 0.4)',
    }
  },
  'bridgewatch': {
    name: 'Bridgewatch',
    emoji: '🏜️',
    description: 'Desert oasis. Goldenrod + sand. Ancient secrets.',
    category: 'albion',
    colors: {
      '--bg-primary': 'rgba(25, 20, 12, 0.95)',
      '--bg-secondary': 'rgba(40, 32, 20, 0.85)',
      '--bg-tertiary': 'rgba(55, 45, 28, 0.75)',
      '--bg-message': 'rgba(68, 55, 35, 0.6)',
      '--border-color': 'rgba(230, 190, 80, 0.25)',
      '--border-glow': 'rgba(230, 190, 80, 0.4)',
      '--text-primary': '#fff0d8',
      '--text-secondary': '#e0c8a0',
      '--text-muted': '#a08860',
      '--accent-primary': '#e6be50',
      '--accent-secondary': '#d4a574',
      '--accent-glow': '#e6be50',
      '--status-online': '#c8e650',
      '--status-offline': '#887050',
      '--scrollbar-thumb': 'rgba(230, 190, 80, 0.35)',
      '--scrollbar-track': 'rgba(40, 32, 20, 0.4)',
    }
  },
  'martlock': {
    name: 'Martlock',
    emoji: '💀',
    description: 'Highland city. Blue steel + teal. War never ends.',
    category: 'albion',
    colors: {
      '--bg-primary': 'rgba(10, 18, 25, 0.95)',
      '--bg-secondary': 'rgba(18, 30, 40, 0.85)',
      '--bg-tertiary': 'rgba(25, 42, 55, 0.75)',
      '--bg-message': 'rgba(32, 52, 68, 0.6)',
      '--border-color': 'rgba(70, 160, 200, 0.25)',
      '--border-glow': 'rgba(70, 160, 200, 0.4)',
      '--text-primary': '#d8ecf8',
      '--text-secondary': '#a0c0d8',
      '--text-muted': '#608098',
      '--accent-primary': '#46a0c8',
      '--accent-secondary': '#68b8d8',
      '--accent-glow': '#46a0c8',
      '--status-online': '#46c8a0',
      '--status-offline': '#506878',
      '--scrollbar-thumb': 'rgba(70, 160, 200, 0.35)',
      '--scrollbar-track': 'rgba(18, 30, 40, 0.4)',
    }
  },
  'thetford': {
    name: 'Thetford',
    emoji: '🌊',
    description: 'Swamp city. Dark orchid purple. Poison and decay.',
    category: 'albion',
    colors: {
      '--bg-primary': 'rgba(18, 12, 25, 0.95)',
      '--bg-secondary': 'rgba(28, 20, 40, 0.85)',
      '--bg-tertiary': 'rgba(40, 28, 55, 0.75)',
      '--bg-message': 'rgba(50, 35, 68, 0.6)',
      '--border-color': 'rgba(160, 100, 220, 0.25)',
      '--border-glow': 'rgba(160, 100, 220, 0.4)',
      '--text-primary': '#ecd8ff',
      '--text-secondary': '#c0a0d8',
      '--text-muted': '#806898',
      '--accent-primary': '#a064dc',
      '--accent-secondary': '#c898e8',
      '--accent-glow': '#a064dc',
      '--status-online': '#a0ff64',
      '--status-offline': '#685878',
      '--scrollbar-thumb': 'rgba(160, 100, 220, 0.35)',
      '--scrollbar-track': 'rgba(28, 20, 40, 0.4)',
    }
  },
  'royal': {
    name: 'Royal',
    emoji: '👑',
    description: 'Crown of Albion. Gold + navy. Regal authority.',
    category: 'albion',
    colors: {
      '--bg-primary': 'rgba(15, 18, 30, 0.95)',
      '--bg-secondary': 'rgba(25, 30, 48, 0.85)',
      '--bg-tertiary': 'rgba(35, 42, 65, 0.75)',
      '--bg-message': 'rgba(45, 52, 80, 0.6)',
      '--border-color': 'rgba(255, 200, 60, 0.25)',
      '--border-glow': 'rgba(255, 200, 60, 0.4)',
      '--text-primary': '#fff0d0',
      '--text-secondary': '#e0c8a0',
      '--text-muted': '#a08858',
      '--accent-primary': '#ffc83c',
      '--accent-secondary': '#6488c8',
      '--accent-glow': '#ffc83c',
      '--status-online': '#78c850',
      '--status-offline': '#786848',
      '--scrollbar-thumb': 'rgba(255, 200, 60, 0.35)',
      '--scrollbar-track': 'rgba(25, 30, 48, 0.4)',
    }
  },

  // === CLASSIC ===
  'dark-mocha': {
    name: 'Dark (Mocha)',
    emoji: '🌑',
    description: 'Catppuccin Mocha. Warm dark, easy on the eyes.',
    category: 'classic',
    colors: {
      '--bg-primary': 'rgba(30, 30, 46, 0.95)',
      '--bg-secondary': 'rgba(40, 40, 60, 0.85)',
      '--bg-tertiary': 'rgba(50, 50, 75, 0.75)',
      '--bg-message': 'rgba(60, 60, 90, 0.6)',
      '--border-color': 'rgba(140, 170, 238, 0.25)',
      '--border-glow': 'rgba(140, 170, 238, 0.4)',
      '--text-primary': '#cdd6f4',
      '--text-secondary': '#a6adc8',
      '--text-muted': '#6c7086',
      '--accent-primary': '#8caaee',
      '--accent-secondary': '#a6e3a1',
      '--accent-glow': '#8caaee',
      '--status-online': '#a6e3a1',
      '--status-offline': '#6c7086',
      '--scrollbar-thumb': 'rgba(140, 170, 238, 0.35)',
      '--scrollbar-track': 'rgba(40, 40, 60, 0.4)',
    }
  },
  'light': {
    name: 'Light',
    emoji: '☀️',
    description: 'Clean daylight. For the masochists who play at noon.',
    category: 'classic',
    colors: {
      '--bg-primary': 'rgba(240, 240, 245, 0.95)',
      '--bg-secondary': 'rgba(250, 250, 252, 0.85)',
      '--bg-tertiary': 'rgba(255, 255, 255, 0.75)',
      '--bg-message': 'rgba(255, 255, 255, 0.7)',
      '--border-color': 'rgba(60, 60, 120, 0.2)',
      '--border-glow': 'rgba(60, 60, 120, 0.35)',
      '--text-primary': '#1e1e2e',
      '--text-secondary': '#45475a',
      '--text-muted': '#6c7086',
      '--accent-primary': '#1e66f5',
      '--accent-secondary': '#40a02b',
      '--accent-glow': '#1e66f5',
      '--status-online': '#40a02b',
      '--status-offline': '#9ca0b0',
      '--scrollbar-thumb': 'rgba(60, 60, 120, 0.3)',
      '--scrollbar-track': 'rgba(240, 240, 245, 0.4)',
    }
  },
};

export const themeCategories = {
  synthwave: { name: 'Synthwave', emoji: '🌆' },
  albion: { name: 'Albion Cities', emoji: '⚔️' },
  classic: { name: 'Classic', emoji: '🎨' },
};

/** @typedef {{ name: string, emoji: string, description: string, category: string, colors: Record<string, string> }} Theme */

/** @type {Record<string, Theme>} */
const themeIndex = themes;

/** @param {string} themeId */
export function getTheme(themeId) {
  return themeIndex[themeId] || themeIndex['synthwave-84'];
}

/** @param {string} themeId */
export function applyTheme(themeId) {
  const theme = getTheme(themeId);
  const root = document.documentElement;
  
  for (const [property, value] of Object.entries(theme.colors)) {
    root.style.setProperty(property, value);
  }
  
  // Store preference
  localStorage.setItem('albion-translator-theme', themeId);
}

export function getStoredTheme() {
  return localStorage.getItem('albion-translator-theme') || 'synthwave-84';
}
