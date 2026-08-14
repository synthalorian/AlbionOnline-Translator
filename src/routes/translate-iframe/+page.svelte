<script>
  import { onMount } from "svelte";
  import { loadSettings, saveSettings } from "$lib/settings.js";
  import { applyTheme, getStoredTheme } from "$lib/themes.js";
  import { languages } from "$lib/languages.js";

  // Defaults come from the shared settings store (same origin, same localStorage)
  const stored = loadSettings();
  let targetLang = $state(stored.targetLanguage || "en");
  let inputText = $state("");
  let result = $state("");
  let translating = $state(false);
  let error = $state("");
  let copied = $state(false);

  // CSS vars don't cross iframe boundaries — apply the stored theme to THIS document
  // before first paint (module script runs pre-paint). The parent also pushes fresh
  // theme colors via postMessage whenever the user switches themes.
  try {
    applyTheme(getStoredTheme());
  } catch (e) {
    console.error("iframe theme apply failed:", e);
  }

  onMount(() => {
    // Parent pushes the active theme whenever it changes
    /** @param {MessageEvent} event */
    const handleTheme = (event) => {
      if (!event.data || event.data.type !== "albion-theme") return;
      const { colors } = event.data;
      if (colors) {
        const root = document.documentElement;
        for (const [property, value] of Object.entries(colors)) {
          root.style.setProperty(property, value);
        }
      }
    };

    window.addEventListener("message", handleTheme);
    return () => window.removeEventListener("message", handleTheme);
  });

  function changeLang() {
    // Keep the shared settings store in sync so the picker survives reloads
    saveSettings({ ...loadSettings(), targetLanguage: targetLang });
    // Tell the parent to update its own select + the backend target language
    window.parent.postMessage({ type: "albion-set-target-lang", lang: targetLang }, "*");
  }

  async function translate() {
    const text = inputText.trim();
    if (!text || translating) return;
    translating = true;
    result = "";
    copied = false;
    error = "";

    const requestId = crypto.randomUUID();

    // Sandboxed iframe has no __TAURI_INTERNALS__ — round-trip through the parent
    window.parent.postMessage(
      { type: "albion-translate", id: requestId, text, targetLang },
      "*"
    );

    /** @param {MessageEvent} event */
    const handler = (event) => {
      if (event.data?.type === "albion-translate-result" && event.data.id === requestId) {
        window.removeEventListener("message", handler);
        if (event.data.error) {
          error = event.data.error;
        } else if (event.data.result) {
          result = event.data.result;
        }
        translating = false;
      }
    };
    window.addEventListener("message", handler);

    // Timeout fallback so the button never spins forever
    setTimeout(() => {
      window.removeEventListener("message", handler);
      if (translating) {
        error = "Translation timed out — check your connection";
        translating = false;
      }
    }, 15000);
  }

  async function copyResult() {
    try {
      await navigator.clipboard.writeText(result);
      copied = true;
      setTimeout(() => (copied = false), 1200);
    } catch (e) {
      error = "Copy failed — select the text manually";
    }
  }
</script>

<div class="iframe-root">
  <div class="iframe-header">
    <span class="iframe-title">User Translator</span>
    <select class="iframe-lang" bind:value={targetLang} onchange={changeLang}>
      {#each languages as lang}
        <option value={lang.code}>{lang.name}</option>
      {/each}
    </select>
  </div>

  <div class="iframe-input-row">
    <textarea
      class="iframe-input"
      rows="1"
      placeholder="Type text to translate… (Enter to translate)"
      bind:value={inputText}
      onkeydown={(e) => {
        if (e.key === "Enter" && !e.shiftKey && !translating) {
          e.preventDefault();
          translate();
        }
      }}
    ></textarea>
    <button
      class="iframe-btn {translating ? 'loading' : ''}"
      onclick={translate}
      disabled={translating || !inputText.trim()}
    >
      {translating ? "Translating…" : "Translate"}
    </button>
  </div>

  {#if error}
    <div class="iframe-error">Error: {error}</div>
  {/if}

  {#if result}
    <div class="iframe-result">
      <span class="iframe-label">Translation:</span>
      <span class="iframe-text">{result}</span>
      <button class="iframe-copy" onclick={copyResult} title="Copy translation">
        {copied ? "✓ Copied" : "Copy"}
      </button>
    </div>
  {/if}
</div>

<style>
  .iframe-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 8px 10px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-family: "Segoe UI", system-ui, sans-serif;
    font-size: 13px;
    gap: 6px;
  }
  .iframe-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .iframe-title {
    font-weight: 600;
    font-size: 12px;
    letter-spacing: 0.5px;
    color: var(--accent-primary);
  }
  .iframe-lang {
    appearance: none;
    -webkit-appearance: none;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    color: var(--text-primary);
    padding: 2px 22px 2px 6px;
    font-size: 11px;
    cursor: pointer;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M0 0l5 6 5-6z' fill='%23f0e0ff'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 6px center;
  }
  .iframe-lang:focus {
    border-color: var(--border-glow);
  }
  .iframe-input-row {
    display: flex;
    gap: 6px;
    align-items: flex-start;
  }
  .iframe-input {
    flex: 1;
    padding: 6px 10px;
    border: 1px solid var(--border-color);
    border-radius: 6px;
    background: var(--bg-tertiary);
    color: var(--text-primary);
    font-size: 12px;
    font-family: inherit;
    outline: none;
    resize: none;
    transition: border-color 0.15s;
  }
  .iframe-input:focus {
    border-color: var(--border-glow);
  }
  .iframe-input::placeholder {
    color: var(--text-muted);
  }
  .iframe-btn {
    padding: 6px 12px;
    border: none;
    border-radius: 6px;
    background: var(--accent-primary);
    color: var(--bg-primary);
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
    transition: opacity 0.15s;
    white-space: nowrap;
  }
  .iframe-btn:hover:not(:disabled) { opacity: 0.85; }
  .iframe-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .iframe-btn.loading { opacity: 0.7; }
  .iframe-result {
    position: relative;
    padding: 6px 10px;
    background: var(--bg-tertiary);
    border-radius: 6px;
    border-left: 3px solid var(--accent-primary);
    font-size: 12px;
    line-height: 1.4;
    word-break: break-word;
    padding-right: 52px;
  }
  .iframe-label {
    color: var(--text-muted);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-right: 6px;
  }
  .iframe-text { color: var(--text-primary); }
  .iframe-copy {
    position: absolute;
    top: 4px;
    right: 4px;
    padding: 2px 8px;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    background: var(--bg-secondary);
    color: var(--text-secondary);
    font-size: 10px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .iframe-copy:hover {
    border-color: var(--border-glow);
    color: var(--text-primary);
  }
  .iframe-error {
    padding: 6px 10px;
    background: rgba(255, 60, 60, 0.15);
    border: 1px solid rgba(255, 60, 60, 0.3);
    border-radius: 6px;
    color: #ff6b6b;
    font-size: 11px;
  }
</style>
