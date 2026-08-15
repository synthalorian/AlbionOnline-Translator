<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { themes, themeCategories, getTheme, applyTheme, getStoredTheme } from "$lib/themes.js";
  import { loadSettings, saveSettings, applySettings } from "$lib/settings.js";
  import { loadLicenseStatus } from "$lib/license.js";
  import { languages } from "$lib/languages.js";
  import LicenseGate from "$lib/LicenseGate.svelte";

  let settings = $state(loadSettings());
  let isCapturing = $state(false);

  /** @typedef {{ timestamp: string, channel: string, sender: string, text: string, source_lang?: string, translated_text?: string }} ChatMessage */
  /** @type {ChatMessage[]} */
  let messages = $state([]);
  /** @type {(() => void) | null} */
  let unlisten = null;
  /** @type {(() => void) | null} */
  let unlistenLocked = null;
  let currentTheme = $state(settings.theme || getStoredTheme());
  let showThemePicker = $state(false);
  let showSettings = $state(false);
  /** @type {import("$lib/license.js").LicenseStatus | null} */
  let license = $state(null);
  /** @type {HTMLIFrameElement | null} */
  let userIframe = $state(null);

  // ── Iframe postMessage bridge: handle translate requests from translate-iframe ──
  /** @param {MessageEvent} event */
  function handleIframeMessage(event) {
    if (!event.data) return;

    // Iframe changed its target language — keep parent select + backend in sync
    if (event.data.type === "albion-set-target-lang") {
      settings.targetLanguage = event.data.lang;
      saveSettings(settings);
      invoke("set_target_language", { lang: event.data.lang }).catch((e) =>
        console.error("Failed to set target language from iframe:", e)
      );
      return;
    }

    if (event.data.type !== "albion-translate") return;
    const { id, text, targetLang } = event.data;

    // invoke can throw synchronously outside Tauri (or on a bad payload) —
    // always answer the iframe so it never hangs waiting on its 15s timeout
    let promise;
    try {
      promise = invoke("translate_user_text", { text, sourceLang: null, targetLang });
    } catch (e) {
      promise = Promise.reject(e);
    }

    // Respond on the IFRAME's window — window.postMessage broadcast does NOT
    // reach same-origin iframes (verified empirically; postMessage only delivers
    // to the target window object it's called on)
    /** @type {(payload: { result?: string, error?: string }) => void} */
    const reply = (payload) => {
      userIframe?.contentWindow?.postMessage(
        { type: "albion-translate-result", id, ...payload },
        "*"
      );
    };

    promise
      .then((result) => reply({ result }))
      .catch((e) => reply({ error: String(e) }));
  }

  // Push current theme colors into the sandboxed iframe (CSS vars don't cross frames)
  function pushThemeToIframe() {
    const theme = getTheme(currentTheme);
    if (!userIframe?.contentWindow) return;
    userIframe.contentWindow.postMessage({ type: "albion-theme", colors: theme.colors }, "*");
  }

  onMount(async () => {
    applyTheme(currentTheme);
    applySettings(settings);

    // Iframe bridge first — independent of Tauri listen/invoke state, so the
    // translator chatbox keeps working even if a capture listener fails to attach
    window.addEventListener("message", handleIframeMessage);

    try {
      isCapturing = await invoke("get_capture_status");
      settings.targetLanguage = await invoke("get_target_language");
      license = await loadLicenseStatus();
    } catch (e) {
      console.error("Failed to load state:", e);
    }

    unlisten = await listen("chat-message", (event) => {
      const msg = event.payload;
      messages = [...messages.slice(-settings.maxMessages + 1), msg];
    });

    // Backend drops messages while locked; re-check status when told
    unlistenLocked = await listen("license-locked", async () => {
      license = await loadLicenseStatus();
    });
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    if (unlistenLocked) unlistenLocked();
    window.removeEventListener("message", handleIframeMessage);
  });

  /** @param {import("$lib/license.js").LicenseStatus} s */
  function onLicenseActivated(s) {
    license = s;
  }

  const FILTER_CHANNELS = ["Say", "Whisper", "Party", "Guild", "Alliance", "Global", "English", "Trade", "LFG", "Recruitment", "Faction", "Unknown"];

  /** @param {string} channel */
  function toggleChannelFilter(channel) {
    settings.channelFilters[channel] = !settings.channelFilters[channel];
    saveSettings(settings);
  }

  /** @param {boolean} on */
  function setAllChannelFilters(on) {
    for (const ch of FILTER_CHANNELS) settings.channelFilters[ch] = on;
    saveSettings(settings);
  }

  /** @param {ChatMessage[]} list */
  function visibleMessages(list) {
    return list.filter((m) => settings.channelFilters[m.channel] !== false);
  }

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
      await invoke("set_target_language", { lang: settings.targetLanguage });
      saveSettings(settings);
    } catch (e) {
      console.error("Language change failed:", e);
    }
  }

  // ── Searchable language picker ──
  let langOpen = $state(false);
  let langQuery = $state("");
  /** @type {HTMLInputElement | null} */
  let langSearchInput = $state(null);

  /** @returns {string} Display name for the current target language */
  function currentLangName() {
    return (
      languages.find((l) => l.code === settings.targetLanguage)?.name ??
      settings.targetLanguage
    );
  }

  const filteredLanguages = $derived(
    languages.filter((l) => {
      const q = langQuery.trim().toLowerCase();
      if (!q) return true;
      return (
        l.name.toLowerCase().includes(q) || l.code.toLowerCase().includes(q)
      );
    })
  );

  function openLangPicker() {
    langQuery = "";
    langOpen = true;
    // Focus the search box after the panel renders
    requestAnimationFrame(() => langSearchInput?.focus());
  }

  /** @param {string} code */
  function selectLang(code) {
    settings.targetLanguage = code;
    langOpen = false;
    changeLanguage();
  }

  /** @param {KeyboardEvent} e */
  function onLangKeydown(e) {
    if (e.key === "Escape") {
      langOpen = false;
    } else if (e.key === "Enter" && filteredLanguages.length > 0) {
      selectLang(filteredLanguages[0].code);
    }
  }

  function clearMessages() {
    messages = [];
  }

  /** @param {string} themeId */
  function selectTheme(themeId) {
    currentTheme = themeId;
    settings.theme = themeId;
    applyTheme(themeId);
    saveSettings(settings);
    pushThemeToIframe();
    showThemePicker = false;
  }

  function updateSettings() {
    applySettings(settings);
    saveSettings(settings);
  }

  /** @param {string} channel */
  function getChannelColor(channel) {
    /** @type {Record<string, string>} */
    const colors = {
      Say: "var(--text-primary)",
      Whisper: "#ff69b4",
      Party: "#4fc3f7",
      Guild: "var(--status-online)",
      Alliance: "#ba68c8",
      Global: "#ffb74d",
      English: "#ffd54f",
      Trade: "#fff176",
      LFG: "#a1887f",
      Recruitment: "#4db6ac",
      Faction: "#e57373",
      Unknown: "var(--text-muted)",
    };
    return colors[channel] || colors.Unknown;
  }
</script>

<main class="overlay-container" style="opacity: {settings.opacity}">
  <!-- Title bar -->
  <div class="title-bar">
    <div class="title">
      <span class="logo">⚔️📜</span>
      <span>Albion Translator</span>
    </div>
    <div class="controls">
      <button class="btn-icon" onclick={() => { showSettings = !showSettings; showThemePicker = false; }} title="Settings">
        ⚙️
      </button>
      <button class="btn-icon" onclick={() => { showThemePicker = !showThemePicker; showSettings = false; }} title="Themes">
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

  <!-- License gate: trial banner or full paywall overlay -->
  {#if license && license.mode !== "licensed"}
    <LicenseGate status={license} onactivated={onLicenseActivated} />
  {/if}

  <!-- Settings panel -->
  {#if showSettings}
    <div class="settings-panel">
      <div class="settings-header">
        <span>Settings</span>
        <button class="btn-icon" onclick={() => showSettings = false}>✕</button>
      </div>
      
      <div class="setting-group">
        <label>
          <span>Opacity</span>
          <span class="setting-value">{Math.round(settings.opacity * 100)}%</span>
        </label>
        <input 
          type="range" 
          min="0.3" 
          max="1" 
          step="0.05" 
          bind:value={settings.opacity}
          oninput={updateSettings}
        />
      </div>

      <div class="setting-group">
        <label>
          <span>Font Size</span>
          <span class="setting-value">{settings.fontSize}px</span>
        </label>
        <input 
          type="range" 
          min="10" 
          max="20" 
          step="1" 
          bind:value={settings.fontSize}
          oninput={updateSettings}
        />
      </div>

      <div class="setting-group">
        <label>
          <span>Max Messages</span>
          <span class="setting-value">{settings.maxMessages}</span>
        </label>
        <input 
          type="range" 
          min="10" 
          max="500" 
          step="10" 
          bind:value={settings.maxMessages}
          oninput={updateSettings}
        />
      </div>

      <div class="setting-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={settings.showTimestamps} onchange={updateSettings} />
          <span>Show Timestamps</span>
        </label>
      </div>

      <div class="setting-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={settings.showOriginal} onchange={updateSettings} />
          <span>Show Original</span>
        </label>
      </div>

      <div class="setting-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={settings.showTranslated} onchange={updateSettings} />
          <span>Show Translated</span>
        </label>
      </div>
    </div>
  {/if}

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
    <div class="lang-picker">
      <button
        class="lang-select"
        onclick={() => (langOpen ? (langOpen = false) : openLangPicker())}
        title="Target language (click to search)"
      >
        {currentLangName()}
      </button>
      {#if langOpen}
        <div class="lang-panel">
          <input
            bind:this={langSearchInput}
            bind:value={langQuery}
            class="lang-search"
            placeholder="Search language…"
            onkeydown={onLangKeydown}
          />
          <div class="lang-list">
            {#if filteredLanguages.length === 0}
              <div class="lang-empty">No matches</div>
            {:else}
              {#each filteredLanguages as lang (lang.code)}
                <button
                  class="lang-item {lang.code === settings.targetLanguage ? 'active' : ''}"
                  onclick={() => selectLang(lang.code)}
                >
                  <span class="lang-item-name">{lang.name}</span>
                  <span class="lang-item-code">{lang.code}</span>
                </button>
              {/each}
            {/if}
          </div>
        </div>
      {/if}
    </div>
  </div>

  <!-- Channel filter bar -->
  <div class="filter-bar">
    {#each FILTER_CHANNELS as ch}
      <button
        class="chip {settings.channelFilters[ch] !== false ? 'on' : 'off'}"
        style="--chip-color: {getChannelColor(ch)}"
        onclick={() => toggleChannelFilter(ch)}
        title="{settings.channelFilters[ch] !== false ? 'Hide' : 'Show'} {ch} chat"
      >
        {ch}
      </button>
    {/each}
    <button class="chip util" onclick={() => setAllChannelFilters(true)} title="Show all channels">All</button>
    <button class="chip util" onclick={() => setAllChannelFilters(false)} title="Hide all channels">None</button>
  </div>

  <!-- User translator iframe -->
  <iframe
    src="/translate-iframe"
    bind:this={userIframe}
    class="user-translate-iframe"
    title="User translator"
    sandbox="allow-same-origin allow-scripts allow-forms"
    onload={pushThemeToIframe}
  ></iframe>

  <!-- Chat messages -->
  <div class="chat-container" style="font-size: {settings.fontSize}px">
    {#if visibleMessages(messages).length === 0}
      <div class="empty-state">
        <p>{messages.length === 0 ? "No messages yet" : "All messages filtered out"}</p>
        <p class="hint">{messages.length === 0 ? "Start capture and chat in Albion to see translations" : "Enable channels in the filter bar above"}</p>
      </div>
    {:else}
      {#each visibleMessages(messages) as msg}
        <div class="message" style="border-left-color: {getChannelColor(msg.channel)}">
          <div class="message-header">
            {#if settings.showTimestamps}
              <span class="timestamp">{msg.timestamp}</span>
            {/if}
            <span class="channel" style="color: {getChannelColor(msg.channel)}">
              [{msg.channel}]
            </span>
            <span class="sender">{msg.sender}</span>
            {#if msg.source_lang && msg.source_lang !== settings.targetLanguage}
              <span class="lang-badge">{msg.source_lang}</span>
            {/if}
          </div>
          <div class="message-body">
            {#if settings.showOriginal}
              <p class="original">{msg.text}</p>
            {/if}
            {#if settings.showTranslated && msg.translated_text && msg.translated_text !== msg.text}
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

  .user-translate-iframe {
    width: 100%;
    height: 140px;
    border: none;
    border-top: 1px solid var(--border-color);
    border-bottom: 1px solid var(--border-color);
    background: var(--bg-secondary);
    flex-shrink: 0;
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

  /* Settings panel */
  .settings-panel {
    position: absolute;
    top: 50px;
    right: 10px;
    width: 260px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 10px;
    padding: 14px;
    z-index: 100;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }

  .settings-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 14px;
    font-weight: 600;
    font-size: 14px;
    color: var(--accent-primary);
  }

  .setting-group {
    margin-bottom: 14px;
  }

  .setting-group label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .setting-value {
    color: var(--accent-primary);
    font-weight: 600;
    font-family: "Consolas", monospace;
  }

  .setting-group input[type="range"] {
    width: 100%;
    height: 6px;
    -webkit-appearance: none;
    background: var(--bg-tertiary);
    border-radius: 3px;
    outline: none;
  }

  .setting-group input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    background: var(--accent-primary);
    border-radius: 50%;
    cursor: pointer;
    box-shadow: 0 0 6px var(--accent-glow);
  }

  .setting-row {
    margin-bottom: 10px;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-primary);
    cursor: pointer;
  }

  .checkbox-label input[type="checkbox"] {
    width: 16px;
    height: 16px;
    accent-color: var(--accent-primary);
    cursor: pointer;
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

  .lang-picker {
    position: relative;
  }

  .lang-select {
    appearance: none;
    -webkit-appearance: none;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    color: var(--text-primary);
    padding: 4px 24px 4px 8px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    outline: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M0 0l5 6 5-6z' fill='%23f0e0ff'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 8px center;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .lang-select:hover {
    border-color: var(--border-glow);
  }

  .lang-panel {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 220px;
    max-height: 320px;
    display: flex;
    flex-direction: column;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
    z-index: 200;
    overflow: hidden;
  }

  .lang-search {
    appearance: none;
    -webkit-appearance: none;
    background: var(--bg-primary);
    border: none;
    border-bottom: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 8px 10px;
    font-size: 12px;
    font-family: inherit;
    outline: none;
  }

  .lang-search:focus {
    border-bottom-color: var(--border-glow);
  }

  .lang-list {
    overflow-y: auto;
    flex: 1;
  }

  .lang-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: 12px;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
  }

  .lang-item:hover {
    background: var(--bg-secondary);
  }

  .lang-item.active {
    color: var(--accent-primary);
    font-weight: 600;
  }

  .lang-item-code {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
  }

  .lang-empty {
    padding: 10px;
    font-size: 12px;
    color: var(--text-muted);
    text-align: center;
  }

  .filter-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 6px 8px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-color);
  }

  .chip {
    border: 1px solid var(--chip-color, var(--border-color));
    background: transparent;
    color: var(--chip-color, var(--text-muted));
    border-radius: 10px;
    padding: 2px 8px;
    font-size: 10px;
    font-weight: 600;
    cursor: pointer;
    opacity: 1;
    transition: opacity 0.15s, background 0.15s;
  }

  .chip.on {
    background: var(--chip-color, var(--accent-primary));
    color: var(--bg-primary);
  }

  .chip.off {
    opacity: 0.35;
  }

  .chip.util {
    border-color: var(--border-color);
    color: var(--text-muted);
  }

  .chip.util:hover {
    color: var(--text-primary);
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
