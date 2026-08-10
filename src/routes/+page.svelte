<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";

  let isCapturing = $state(false);
  let targetLang = $state("en");
  let messages = $state([]);
  let unlisten = null;
  let maxMessages = 100;

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

  function getChannelColor(channel) {
    const colors = {
      Say: "#e0e0e0",
      Whisper: "#ff69b4",
      Party: "#4fc3f7",
      Guild: "#81c784",
      Alliance: "#ba68c8",
      Global: "#ffb74d",
      Trade: "#fff176",
      LFG: "#a1887f",
      Unknown: "#9e9e9e",
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
    background: rgba(15, 15, 25, 0.92);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(100, 100, 255, 0.2);
    border-radius: 12px;
    overflow: hidden;
    color: #e0e0e0;
  }

  .title-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 14px;
    background: rgba(30, 30, 50, 0.8);
    border-bottom: 1px solid rgba(100, 100, 255, 0.15);
    -webkit-app-region: drag;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    font-size: 14px;
    color: #b0b0ff;
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
    background: rgba(60, 60, 100, 0.5);
    border: 1px solid rgba(100, 100, 255, 0.2);
    border-radius: 6px;
    padding: 4px 8px;
    cursor: pointer;
    font-size: 12px;
    transition: all 0.2s;
    color: #e0e0e0;
  }

  .btn-icon:hover {
    background: rgba(80, 80, 140, 0.7);
    border-color: rgba(100, 100, 255, 0.4);
  }

  .btn-icon.active {
    background: rgba(255, 80, 80, 0.3);
    border-color: rgba(255, 80, 80, 0.5);
  }

  .status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 14px;
    background: rgba(25, 25, 40, 0.6);
    border-bottom: 1px solid rgba(100, 100, 255, 0.1);
    font-size: 12px;
  }

  .status {
    font-weight: 600;
    letter-spacing: 0.5px;
  }

  .status.online {
    color: #4caf50;
  }

  .status.offline {
    color: #9e9e9e;
  }

  .lang-select {
    background: rgba(40, 40, 70, 0.8);
    border: 1px solid rgba(100, 100, 255, 0.2);
    border-radius: 6px;
    color: #e0e0e0;
    padding: 4px 8px;
    font-size: 12px;
    cursor: pointer;
    outline: none;
  }

  .lang-select:focus {
    border-color: rgba(100, 100, 255, 0.5);
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
    background: rgba(30, 30, 50, 0.3);
  }

  .chat-container::-webkit-scrollbar-thumb {
    background: rgba(100, 100, 255, 0.3);
    border-radius: 3px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #666;
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
    background: rgba(30, 30, 50, 0.5);
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
    color: #666;
    font-family: "Consolas", monospace;
  }

  .channel {
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .sender {
    color: #b0b0ff;
    font-weight: 600;
  }

  .lang-badge {
    background: rgba(100, 100, 255, 0.2);
    border: 1px solid rgba(100, 100, 255, 0.3);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 10px;
    text-transform: uppercase;
  }

  .message-body {
    font-size: 13px;
    line-height: 1.4;
  }

  .original {
    color: #e0e0e0;
    word-break: break-word;
  }

  .translated {
    color: #81c784;
    margin-top: 4px;
    padding-top: 4px;
    border-top: 1px dashed rgba(100, 255, 100, 0.2);
    word-break: break-word;
  }

  .translated::before {
    content: "→ ";
    color: #4caf50;
  }

  .footer {
    padding: 6px 14px;
    background: rgba(20, 20, 35, 0.8);
    border-top: 1px solid rgba(100, 100, 255, 0.1);
    text-align: center;
  }

  .disclaimer {
    font-size: 10px;
    color: #555;
    letter-spacing: 0.3px;
  }
</style>
