<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { themes, themeCategories, applyTheme, getStoredTheme } from "$lib/themes.js";

  let isCapturing = $state(false);
  let targetLang = $state("en");
  let messages = $state([]);
  let unlisten = null;
  let maxMessages = 100;
  let currentTheme = $state(getStoredTheme());
  let showThemePicker = $state(false);

  const languages = [
    { code: "en", name: "English" },
    { code: "es", name: "Español" },
    { code: "pt", name: "Português" },
    { code: "zh", name: "中文" },
    { code: "ru", name: "Русский" },
    { code: "de", name: "Deutsch" },
    { code: "fr", name: "Français" },
    { code: "ko", name: "한국어" },
    { code: "ja", name: "日本語" },
    { code: "tr", name: "Türkçe" },
  ];

  onMount(async () => {
    // Apply stored theme
    applyTheme(currentTheme);

    // Load initial state
    try {
      isCapturing = await invoke("get_capture_status");
      targetLang = await invoke("get_target_language");
    } catch (e) {
      console.error("Failed to load state:", e);
    }

    // Listen for chat messages
    unlisten = await listen("chat-message", (event) => {
      const msg = event.payload;
      messages = [...messages.slice(-maxMessages + 1), msg];
    });
  });

  onDestroy(() => {
    if (unlisten) unlisten();
  });

  async function toggleCapture() {
    try {
      if (isCapturing) {
        await invoke("stop_capture");
        isCapturing = false;
      } else {
        await invoke("start_capture");
        isCapturing = true;
      }
    } catch (e) {
      console.error("Capture toggle failed:", e);
      alert("Failed to toggle capture: " + e);
    }
  }

  async function changeLanguage() {
    try {
      await invoke("set_target_language", { lang: targetLang });
    } catch (e) {
      console.error("Language change failed:", e);
    }
  }

  function clearMessages() {
    messages = [];
  }

  function selectTheme(themeId) {
    currentTheme = themeId;
    applyTheme(themeId);
    showThemePicker = false;
  }

  function getChannelColor(channel) {
    const colors = {
      Say: "var(--text-primary)",
      Whisper: "#ff69b4",
      Party: "#4fc3f7",
      Guild: "var(--status-online)",
      Alliance: "#ba68c8",
      Global: "#ffb74d",
      Trade: "#fff176",
      LFG: "#a1887f",
      Faction: "#e57373",
      Unknown: "var(--text-muted)",
    };
    return colors[channel] || colors.Unknown;
  }
</script>

<main class="overlay-container">
  <!-- Title bar -->
  <div class="title-bar">
    <div class="title">
      <span class="logo">🎹🦞</span>
      <span>Albion Translator</span>
    </div>
    <div class="controls">
      <button class="btn-icon" onclick={() => showThemePicker = !showThemePicker} title="Themes">
        🎨
      </button>
      <button class="btn-icon" onclick={clearMessages} title="Clear">🗑️</button>
      <button 
        class="btn-icon {isCapturing ? 'active' : ''}" 
        onclick={toggleCapture}
        title={isCapturing ? "Stop Capture" : "Start Capture"}
      >
        {isCapturing ? "⏹️" : "▶️"}
      </button>
    </div>
  </div>

  <!-- Theme picker dropdown -->
  {#if showThemePicker}
    <div class="theme-picker">
      {#each Object.entries(themeCategories) as [catId, category]}
        <div class="theme-category">
          <div class="category-header">
            <span class="category-emoji">{category.emoji}</span>
            <span class="category-name">{category.name}</span>
          </div>
          <div class="theme-grid">
            {#each Object.entries(themes).filter(([_, t]) => t.category === catId) as [themeId, theme]}
              <button
                class="theme-option {currentTheme === themeId ? 'selected' : ''}"
                onclick={() => selectTheme(themeId)}
                title={theme.description}
              >
                <span class="theme-emoji">{theme.emoji}</span>
                <span class="theme-name">{theme.name}</span>
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Status bar -->
  <div class="status-bar">
    <span class="status {isCapturing ? 'online' : 'offline'}">
      {isCapturing ? "● CAPTURING" : "○ IDLE"}
    </span>
    <select bind:value={targetLang} onchange={changeLanguage} class="lang-select">
      {#each languages as lang}
        <option value={lang.code}>{lang.name}</option>
      {/each}
    </select>
  </div>

  <!-- Chat messages -->
  <div class="chat-container">
    {#if messages.length === 0}
      <div class="empty-state">
        <p>No messages yet</p>
        <p class="hint">Start capture and chat in Albion to see translations</p>
      </div>
    {:else}
      {#each messages as msg}
        <div class="message" style="border-left-color: {getChannelColor(msg.channel)}">
          <div class="message-header">
            <span class="timestamp">{msg.timestamp}</span>
            <span class="channel" style="color: {getChannelColor(msg.channel)}">
              [{msg.channel}]
            </span>
            <span class="sender">{msg.sender}</span>
            {#if msg.source_lang && msg.source_lang !== targetLang}
              <span class="lang-badge">{msg.source_lang}</span>
            {/if}
          </div>
          <div class="message-body">
            <p class="original">{msg.text}</p>
            {#if msg.translated_text && msg.translated_text !== msg.text}
              <p class="translated">{msg.translated_text}</p>
            {/if}
          </div>
        </div>
      {/each}
    {/if}
  </div>

  <!-- Footer -->
  <div class="footer">
    <span class="disclaimer">Passive sniffer — no game modification</span>
  </div>
</main>

<style>
  :global(*) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }

  :global(body) {
    font-family: "Segoe UI", system-ui, -apple-system, sans-serif;
    background: transparent;
    overflow: hidden;
    user-select: none;
  }

  .overlay-container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg-primary);
    backdrop-filter: blur(12px);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    overflow: hidden;
    color: var(--text-primary);
    position: relative;
  }

  .title-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 14px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-color);
    -webkit-app-region: drag;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    font-size: 14px;
    color: var(--accent-primary);
  }

  .logo {
    font-size: 16px;
  }

  .controls {
    display: flex;
    gap: 6px;
    -webkit-app-region: no-drag;
  }

  .btn-icon {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 4px 8px;
    cursor: pointer;
    font-size: 12px;
    transition: all 0.2s;
    color: var(--text-primary);
  }

  .btn-icon:hover {
    background: var(--bg-message);
    border-color: var(--border-glow);
  }

  .btn-icon.active {
    background: rgba(255, 80, 80, 0.3);
    border-color: rgba(255, 80, 80, 0.5);
  }

  /* Theme picker */
  .theme-picker {
    position: absolute;
    top: 50px;
    right: 10px;
    width: 280px;
    max-height: 400px;
    overflow-y: auto;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 10px;
    padding: 12px;
    z-index: 100;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }

  .theme-category {
    margin-bottom: 14px;
  }

  .theme-category:last-child {
    margin-bottom: 0;
  }

  .category-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
  }

  .category-emoji {
    font-size: 14px;
  }

  .theme-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .theme-option {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    cursor: pointer;
    font-size: 12px;
    color: var(--text-primary);
    transition: all 0.15s;
    text-align: left;
  }

  .theme-option:hover {
    background: var(--bg-message);
    border-color: var(--border-glow);
    transform: translateY(-1px);
  }

  .theme-option.selected {
    border-color: var(--accent-primary);
    box-shadow: 0 0 8px var(--accent-glow);
  }

  .theme-emoji {
    font-size: 14px;
  }

  .theme-name {
    font-weight: 500;
  }

  .status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 14px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-color);
    font-size: 12px;
  }

  .status {
    font-weight: 600;
    letter-spacing: 0.5px;
  }

  .status.online {
    color: var(--status-online);
  }

  .status.offline {
    color: var(--status-offline);
  }

  .lang-select {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    color: var(--text-primary);
    padding: 4px 8px;
    font-size: 12px;
    cursor: pointer;
    outline: none;
  }

  .lang-select:focus {
    border-color: var(--border-glow);
  }

  .chat-container {
    flex: 1;
    overflow-y: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .chat-container::-webkit-scrollbar {
    width: 6px;
  }

  .chat-container::-webkit-scrollbar-track {
    background: var(--scrollbar-track);
  }

  .chat-container::-webkit-scrollbar-thumb {
    background: var(--scrollbar-thumb);
    border-radius: 3px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted);
    text-align: center;
  }

  .empty-state p {
    margin: 4px 0;
  }

  .hint {
    font-size: 12px;
    opacity: 0.6;
  }

  .message {
    background: var(--bg-message);
    border-left: 3px solid;
    border-radius: 0 8px 8px 0;
    padding: 8px 12px;
    animation: slideIn 0.15s ease-out;
  }

  @keyframes slideIn {
    from {
      opacity: 0;
      transform: translateX(-8px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }

  .message-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
    font-size: 11px;
    flex-wrap: wrap;
  }

  .timestamp {
    color: var(--text-muted);
    font-family: "Consolas", monospace;
  }

  .channel {
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .sender {
    color: var(--accent-primary);
    font-weight: 600;
  }

  .lang-badge {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-secondary);
  }

  .message-body {
    font-size: 13px;
    line-height: 1.4;
  }

  .original {
    color: var(--text-primary);
    word-break: break-word;
  }

  .translated {
    color: var(--status-online);
    margin-top: 4px;
    padding-top: 4px;
    border-top: 1px dashed var(--border-color);
    word-break: break-word;
  }

  .translated::before {
    content: "→ ";
    color: var(--status-online);
  }

  .footer {
    padding: 6px 14px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border-color);
    text-align: center;
  }

  .disclaimer {
    font-size: 10px;
    color: var(--text-muted);
    letter-spacing: 0.3px;
  }
</style>
