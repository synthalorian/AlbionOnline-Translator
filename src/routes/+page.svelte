<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { themes, themeCategories, getTheme, applyTheme, getStoredTheme } from "$lib/themes.js";
  import { loadSettings, saveSettings, applySettings } from "$lib/settings.js";
  import { languages } from "$lib/languages.js";

  let settings = $state(loadSettings());
  let isCapturing = $state(false);
  let captureDevice = $state("");
  let packetCount = $state(0);
  let statsTimer = null;

  /** @typedef {{ timestamp: string, channel: string, channel_id: number, sender: string, text: string, source_lang?: string, translated_text?: string }} ChatMessage */
  /** @type {ChatMessage[]} */
  let messages = $state([]);
  /** @type {(() => void) | null} */
  let unlisten = null;
  let currentTheme = $state(settings.theme || getStoredTheme());
  let showThemePicker = $state(false);
  let showSettings = $state(false);
  /** @type {{name: string, label: string, is_tunnel: boolean, ipv4: string}[]} */
  let captureDevices = $state([]);
  let diagRunning = $state(false);
  /** @type {string} */
  let diagResult = $state("");
  let checkingUpdates = $state(false);
  let updateStatus = $state("");
  let compactMode = $state(false);
  /** @type {HTMLIFrameElement | null} */
  let userIframe = $state(null);
  /** @type {HTMLDivElement | null} */
  let chatContainer = $state(null);
  // Scroll freeze: when the user scrolls up, pause auto-scroll. Resumes when
  // they scroll back to the bottom. Classic chat behavior.
  let scrollFrozen = $state(false);

  // Auto-scroll chat to bottom when new messages arrive (unless frozen)
  $effect(() => {
    // Depend on messages.length so this fires on every new message
    void messages.length;
    if (chatContainer && !scrollFrozen) {
      // Use requestAnimationFrame to wait for DOM update
      requestAnimationFrame(() => {
        chatContainer.scrollTop = chatContainer.scrollHeight;
      });
    }
  });

  function onChatScroll() {
    if (!chatContainer) return;
    const { scrollTop, scrollHeight, clientHeight } = chatContainer;
    // Consider "at bottom" if within 30px of the bottom
    const atBottom = scrollHeight - scrollTop - clientHeight < 30;
    scrollFrozen = !atBottom;
  }

  function scrollToBottom() {
    scrollFrozen = false;
    if (chatContainer) {
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }
  }

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
    } catch (e) {
      console.error("Failed to load state:", e);
    }

    unlisten = await listen("chat-message", (event) => {
      const msg = event.payload;
      messages = [...messages.slice(-settings.maxMessages + 1), msg];
    });
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    window.removeEventListener("message", handleIframeMessage);
  });

  const FILTER_CHANNELS = ["Say", "Whisper", "Party", "Guild", "Alliance", "Global", "Trade", "LFG", "Recruitment", "Faction", "Unknown"];

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

  // Collect unique unknown channel ids for the setup banner
  const unknownChannels = $derived(
    [...new Set(
      messages
        .filter((m) => m.channel === "Unknown")
        .map((m) => m.channel_id)
    )]
  );

  async function toggleCapture() {
    try {
      if (isCapturing) {
        await invoke("stop_capture");
        isCapturing = false;
        captureDevice = "";
        packetCount = 0;
        if (statsTimer) { clearInterval(statsTimer); statsTimer = null; }
      } else {
        // Returns e.g. "Capture started on Realtek PCIe GbE (\Device\NPF_{...})"
        const started = await invoke("start_capture", {
          device: settings.captureInterface || null,
        });
        captureDevice = String(started).replace(/^Capture started on\s*/, "");
        packetCount = 0;
        isCapturing = true;
        // Poll packet count — 0 while chatting = wrong interface.
        statsTimer = setInterval(async () => {
          try {
            const [running, count] = await invoke("get_capture_stats");
            packetCount = count;
            if (!running) {
              isCapturing = false;
              captureDevice = "";
              clearInterval(statsTimer);
              statsTimer = null;
            }
          } catch { /* transient — next tick retries */ }
        }, 2000);
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

  /** Tag an Unknown channel as a known type (Party/Guild/Alliance/Trade) */
  /** @param {number} channelId @param {string} channelType */
  async function tagChannel(channelId, channelType) {
    try {
      await invoke("set_channel_mapping", { channelId, channel: channelType });
      // Retroactively update visible messages with this channel_id
      messages = messages.map((m) =>
        m.channel_id === channelId && m.channel === "Unknown"
          ? { ...m, channel: channelType }
          : m
      );
    } catch (e) {
      console.error("Channel tag failed:", e);
    }
  }

  /** Dismiss an unknown channel from the setup banner (hide its messages) */
  /** @param {number} channelId */
  function dismissUnknown(channelId) {
    // Remove messages from this unknown channel so the banner clears
    messages = messages.filter((m) => m.channel_id !== channelId);
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

  async function checkUpdates() {
    checkingUpdates = true;
    updateStatus = "";
    try {
      const result = await invoke("check_for_updates");
      updateStatus = result;
    } catch (e) {
      updateStatus = String(e);
    } finally {
      checkingUpdates = false;
    }
  }

  async function toggleClickThrough() {
    try {
      await invoke("set_click_through", { enabled: settings.clickThrough });
      saveSettings(settings);
    } catch (e) {
      console.error("Click-through toggle failed:", e);
      // Revert on failure
      settings.clickThrough = !settings.clickThrough;
    }
  }

  async function loadCaptureDevices() {
    try {
      captureDevices = await invoke("list_capture_devices");
      // Saved interface vanished (VPN uninstalled, adapter renamed) → auto.
      if (
        settings.captureInterface &&
        !captureDevices.some((d) => d.name === settings.captureInterface)
      ) {
        settings.captureInterface = "";
        saveSettings(settings);
      }
    } catch (e) {
      console.error("Failed to list capture devices:", e);
      captureDevices = [];
    }
  }

  async function changeCaptureInterface() {
    saveSettings(settings);
    // Apply immediately: restart capture on the newly selected interface.
    if (isCapturing) {
      await toggleCapture(); // stop
      await toggleCapture(); // start with new device
    }
  }

  async function runDiagnostic() {
    diagRunning = true;
    diagResult = "";
    try {
      // Stop capture so the diagnostic can open the device.
      const wasCapturing = isCapturing;
      if (wasCapturing) await toggleCapture();
      const r = await invoke("run_network_diagnostic", {
        device: settings.captureInterface || null,
      });
      const ports = r.top_udp_ports.map(([p, n]) => `${p}×${n}`).join(", ");
      let verdict;
      if (r.total_packets === 0) {
        verdict = "❌ No packets at all — capture driver (Npcap) or adapter is broken.";
      } else if (r.albion_packets > 0 && r.photon_chat_decoded > 0) {
        verdict = "✅ Chat decoded during the survey — if the feed stayed empty, it's a UI bug. Report this!";
      } else if (r.albion_packets > 0 && r.albion_encrypted >= r.albion_packets / 2) {
        verdict = "❌ Albion traffic is Photon-ENCRYPTED on this connection — decoder can't read it. Report this!";
      } else if (r.albion_packets > 0 && r.albion_inbound === 0 && r.albion_outbound > 0) {
        verdict = "⚠️ ONE-WAY capture: only OUTBOUND game packets visible — inbound never reaches us. Fully QUIT any VPN/filter software (WARP, antivirus web shields), or reinstall Npcap with 'Support raw 802.11 traffic' checked.";
      } else if (r.albion_packets > 0) {
        verdict = "⚠️ Game traffic arrives but no chat decoded in 10s — chat in-game during the survey and re-run. Still zero = protocol change, report this!";
      } else {
        verdict = "⚠️ Capture works, but no Albion traffic on this adapter — a VPN is likely still routing the game (port 2408 = WARP/WireGuard), or Albion isn't chatting.";
      }
      diagResult =
        `${r.device}\n10s survey: ${r.total_packets} total, ${r.udp_packets} UDP, ` +
        `${r.albion_packets} Albion-port (in ${r.albion_inbound} / out ${r.albion_outbound})\n` +
        `Photon: ${r.albion_encrypted} encrypted, ${r.photon_chat_decoded} chat decoded\n` +
        `Top UDP ports: ${ports || "none"}\n${verdict}`;
      if (wasCapturing) await toggleCapture();
    } catch (e) {
      diagResult = "Diagnostic failed: " + e;
    } finally {
      diagRunning = false;
    }
  }

  function exportChatLog() {
    const lines = messages.map((m) => {
      const translated = m.translated_text && m.translated_text !== m.text
        ? ` → ${m.translated_text}`
        : "";
      return `[${m.timestamp}] [${m.channel}] ${m.sender}: ${m.text}${translated}`;
    });
    const blob = new Blob([lines.join("\n")], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `albion-chat-${new Date().toISOString().slice(0, 19).replace(/:/g, "-")}.txt`;
    a.click();
    URL.revokeObjectURL(url);
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
      <button class="btn-icon" onclick={() => compactMode = !compactMode} title={compactMode ? "Expand" : "Compact mode"}>
        {compactMode ? "🔽" : "🔼"}
      </button>
      <button class="btn-icon" onclick={() => { showSettings = !showSettings; showThemePicker = false; }} title="Settings">
        ⚙️
      </button>
      <button class="btn-icon" onclick={() => { showThemePicker = !showThemePicker; showSettings = false; }} title="Themes">
        🎨
      </button>
      <button class="btn-icon" onclick={clearMessages} title="Clear">🗑️</button>
      <button class="btn-icon" onclick={exportChatLog} title="Export chat log">💾</button>
      <button 
        class="btn-icon {isCapturing ? 'active' : ''}" 
        onclick={toggleCapture}
        title={isCapturing ? "Stop Capture" : "Start Capture"}
      >
        {isCapturing ? "⏹️" : "▶️"}
      </button>
    </div>
  </div>

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

      <div class="setting-row">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={settings.clickThrough} onchange={toggleClickThrough} />
          <span>Click-Through (mouse passes to game)</span>
        </label>
      </div>

      <div class="setting-row">
        <label style="display: block; margin-bottom: 4px;">Capture Interface</label>
        <select
          bind:value={settings.captureInterface}
          onchange={changeCaptureInterface}
          onfocus={loadCaptureDevices}
          style="width: 100%; padding: 6px 8px; background: var(--bg-tertiary); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: 6px;"
        >
          <option value="">Auto-detect (skips VPN tunnels)</option>
          {#each captureDevices as dev}
            <option value={dev.name}>
              {dev.is_tunnel ? "⚠ VPN: " : ""}{dev.label}{dev.ipv4 ? ` — ${dev.ipv4}` : ""}
            </option>
          {/each}
        </select>
        {#if captureDevices.some((d) => d.is_tunnel)}
          <span style="font-size: 10px; color: var(--text-muted);">
            ⚠ VPN adapters can't see game chat — traffic is encrypted inside the tunnel.
            Pick your physical Wi-Fi/Ethernet adapter (and split-tunnel Albion if the VPN stays on).
          </span>
        {/if}
      </div>

      <div class="setting-row">
        <button class="setup-btn" style="width: 100%;" onclick={runDiagnostic} disabled={diagRunning}>
          {diagRunning ? "Surveying network… (10s)" : "Run Network Diagnostic"}
        </button>
        {#if diagResult}
          <pre style="font-size: 10px; color: var(--text-secondary, #aaa); white-space: pre-wrap; margin-top: 6px; user-select: text;">{diagResult}</pre>
        {/if}
      </div>

      <div class="setting-row" style="margin-top: 12px; border-top: 1px solid var(--border-color); padding-top: 12px;">
        <button class="setup-btn" style="width: 100%;" onclick={checkUpdates} disabled={checkingUpdates}>
          {checkingUpdates ? "Checking…" : updateStatus || "Check for Updates"}
        </button>
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
    {#if isCapturing && captureDevice}
      <span class="capture-detail" title="{captureDevice} · {packetCount} pkts">
        <span class="capture-count">{packetCount} pkts</span>
        <span class="capture-name"> · {captureDevice}</span>
      </span>
    {/if}
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

  <!-- Channel setup banner — shown when unknown channels are detected -->
  {#if unknownChannels.length > 0 && !compactMode}
    <div class="setup-banner">
      <div class="setup-title">⚠️ Unknown channels detected — click to identify:</div>
      <div class="setup-channels">
        {#each unknownChannels as chId}
          <div class="setup-channel-row">
            <span class="setup-channel-id">Channel {chId}</span>
            {#each ["Party", "Guild", "Alliance"] as ch}
              <button
                class="setup-btn"
                onclick={() => tagChannel(chId, ch)}
              >{ch}</button>
            {/each}
            <button
              class="setup-btn dismiss"
              onclick={() => dismissUnknown(chId)}
            >Skip</button>
          </div>
        {/each}
      </div>
      <div class="setup-hint">Tip: send a message in each channel in-game, then click the matching label here. Saved for this game session.</div>
    </div>
  {/if}

  <!-- Channel filter bar -->
  {#if !compactMode}
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
  {/if}

  <!-- User translator iframe -->
  {#if !compactMode}
  <iframe
    src="/translate-iframe"
    bind:this={userIframe}
    class="user-translate-iframe"
    title="User translator"
    sandbox="allow-same-origin allow-scripts allow-forms"
    onload={pushThemeToIframe}
  ></iframe>
  {/if}

  <!-- Chat messages -->
  <div class="chat-container" bind:this={chatContainer} onscroll={onChatScroll} style="font-size: {settings.fontSize}px">
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
              [{msg.channel === "Unknown" ? `Unknown:${msg.channel_id}` : msg.channel}]
            </span>
            {#if msg.channel === "Unknown"}
              <span class="channel-tagger">
                {#each ["Party", "Guild", "Alliance", "Trade"] as ch}
                  <button
                    class="tag-btn"
                    onclick={() => tagChannel(msg.channel_id, ch)}
                    title="Tag channel {msg.channel_id} as {ch}"
                  >{ch}</button>
                {/each}
              </span>
            {/if}
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

  {#if scrollFrozen}
    <button class="scroll-bottom-btn" onclick={scrollToBottom}>
      ↓ New messages
    </button>
  {/if}

  <!-- Footer -->
  {#if !compactMode}
  <div class="footer">
    <span class="disclaimer">Passive sniffer — no game modification</span>
  </div>
  {/if}
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

  .capture-detail {
    font-size: 11px;
    color: var(--text-secondary, #888);
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: default;
  }

  /* The count is the diagnostic gold — never let ellipsis eat it. Truncate
     the adapter name instead. (Burned 2026-08-28: long Intel name hid the
     pkts counter behind "…", blocking remote diagnosis.) */
  .capture-count {
    font-weight: 600;
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

  .setup-banner {
    padding: 8px 12px;
    background: rgba(255, 170, 0, 0.1);
    border-bottom: 1px solid rgba(255, 170, 0, 0.3);
  }

  .setup-title {
    font-size: 12px;
    font-weight: 600;
    color: #ffaa00;
    margin-bottom: 6px;
  }

  .setup-channels {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .setup-channel-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .setup-channel-id {
    font-size: 11px;
    color: var(--text-muted);
    font-family: monospace;
    min-width: 100px;
  }

  .setup-btn {
    padding: 3px 10px;
    border: 1px solid var(--accent-primary);
    border-radius: 4px;
    background: transparent;
    color: var(--accent-primary);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .setup-btn:hover {
    background: var(--accent-primary);
    color: var(--bg-primary);
  }

  .setup-btn.dismiss {
    border-color: var(--text-muted);
    color: var(--text-muted);
  }

  .setup-btn.dismiss:hover {
    background: var(--text-muted);
    color: var(--bg-primary);
  }

  .setup-hint {
    font-size: 10px;
    color: var(--text-muted);
    margin-top: 6px;
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

  .scroll-bottom-btn {
    position: absolute;
    bottom: 60px;
    right: 16px;
    padding: 6px 14px;
    background: var(--accent-primary);
    color: var(--bg-primary);
    border: none;
    border-radius: 16px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
    z-index: 50;
    transition: opacity 0.2s;
    animation: slideUp 0.2s ease-out;
  }

  .scroll-bottom-btn:hover {
    opacity: 0.9;
  }

  @keyframes slideUp {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
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

  .channel-tagger {
    display: inline-flex;
    gap: 3px;
    margin-left: 4px;
  }

  .tag-btn {
    background: var(--bg-tertiary);
    border: 1px solid var(--accent-primary);
    border-radius: 4px;
    padding: 1px 6px;
    font-size: 10px;
    color: var(--accent-primary);
    cursor: pointer;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    transition: all 0.15s;
    opacity: 0.7;
  }

  .tag-btn:hover {
    opacity: 1;
    background: var(--accent-primary);
    color: var(--bg-primary);
    box-shadow: 0 0 6px var(--accent-primary);
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
