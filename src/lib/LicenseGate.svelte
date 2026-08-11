<script>
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { activateLicense, deactivateLicense } from "$lib/license.js";

  /**
   * @type {{
   *   status: import("$lib/license.js").LicenseStatus,
   *   onactivated: (s: any) => void,
   * }} */
  let { status, onactivated } = $props();

  let keyInput = $state("");
  let busy = $state(false);
  let error = $state("");
  let showKeyEntry = $state(status.mode === "locked");

  async function buy() {
    try {
      await openUrl(status.buy_url);
    } catch (e) {
      error = "Couldn't open checkout: " + e;
    }
  }

  async function activate() {
    if (!keyInput.trim()) return;
    busy = true;
    error = "";
    try {
      const s = await activateLicense(keyInput);
      onactivated(s);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function fmtTrial(days) {
    if (days <= 0) return "Trial expires today";
    if (days === 1) return "1 day left in trial";
    return `${days} days left in trial`;
  }
</script>

{#if status.mode === "locked"}
  <!-- Hard paywall: covers the entire overlay -->
  <div class="gate-overlay">
    <div class="gate-card">
      <div class="gate-logo">🎹🦞</div>
      <h2>Trial ended</h2>
      <p class="gate-copy">
        Albion Translator is a one-time purchase — no subscription, ever.
        Unlock it once and it's yours.
      </p>
      <button class="btn-buy" onclick={buy}>Unlock — $9.99</button>
      <button class="btn-have-key" onclick={() => (showKeyEntry = !showKeyEntry)}>
        I already have a license key
      </button>
      {#if showKeyEntry}
        <div class="key-entry">
          <input
            type="text"
            bind:value={keyInput}
            placeholder="XXXX-XXXX-XXXX-XXXX"
            onkeydown={(e) => e.key === "Enter" && activate()}
          />
          <button class="btn-activate" onclick={activate} disabled={busy}>
            {busy ? "Checking…" : "Activate"}
          </button>
        </div>
      {/if}
      {#if error}<p class="gate-error">{error}</p>{/if}
    </div>
  </div>
{:else if status.mode === "trial"}
  <!-- Slim trial banner, dismissible into settings -->
  <div class="trial-banner">
    <span>{fmtTrial(status.days_remaining)}</span>
    <button class="btn-buy small" onclick={buy}>Unlock</button>
    {#if showKeyEntry}
      <input
        type="text"
        class="inline-key"
        bind:value={keyInput}
        placeholder="License key"
        onkeydown={(e) => e.key === "Enter" && activate()}
      />
      <button class="btn-activate small" onclick={activate} disabled={busy}>
        {busy ? "…" : "Apply"}
      </button>
    {:else}
      <button class="btn-key-link" onclick={() => (showKeyEntry = true)}>have a key?</button>
    {/if}
  </div>
  {#if error}<div class="trial-error">{error}</div>{/if}
{/if}

<style>
  .gate-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.85);
    backdrop-filter: blur(6px);
  }

  .gate-card {
    background: var(--bg-secondary);
    border: 1px solid var(--accent-primary);
    border-radius: 12px;
    box-shadow: 0 0 24px var(--accent-glow);
    padding: 28px 32px;
    max-width: 340px;
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .gate-logo {
    font-size: 40px;
  }

  .gate-card h2 {
    margin: 0;
    color: var(--text-primary);
    font-size: 20px;
  }

  .gate-copy {
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.5;
    margin: 0;
  }

  .btn-buy {
    background: var(--accent-primary);
    color: var(--bg-primary);
    border: none;
    border-radius: 8px;
    padding: 12px;
    font-size: 15px;
    font-weight: 700;
    cursor: pointer;
    box-shadow: 0 0 10px var(--accent-glow);
  }

  .btn-buy:hover {
    filter: brightness(1.15);
  }

  .btn-buy.small {
    padding: 4px 12px;
    font-size: 12px;
  }

  .btn-have-key,
  .btn-key-link {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
    text-decoration: underline;
  }

  .key-entry {
    display: flex;
    gap: 8px;
  }

  .key-entry input,
  .inline-key {
    flex: 1;
    background: var(--bg-tertiary);
    border: 1px solid var(--accent-primary);
    border-radius: 6px;
    color: var(--text-primary);
    padding: 8px 10px;
    font-size: 13px;
    font-family: monospace;
    min-width: 0;
  }

  .btn-activate {
    background: var(--bg-tertiary);
    color: var(--accent-primary);
    border: 1px solid var(--accent-primary);
    border-radius: 6px;
    padding: 8px 14px;
    font-size: 13px;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-activate.small {
    padding: 3px 10px;
    font-size: 12px;
  }

  .btn-activate:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .gate-error,
  .trial-error {
    color: #e57373;
    font-size: 12px;
    margin: 0;
  }

  .trial-error {
    padding: 4px 10px;
    background: var(--bg-secondary);
  }

  .trial-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--accent-primary);
    color: var(--text-muted);
    font-size: 12px;
  }

  .trial-banner span {
    flex: 1;
  }

  .inline-key {
    flex: 2;
    padding: 4px 8px;
    font-size: 11px;
  }
</style>
