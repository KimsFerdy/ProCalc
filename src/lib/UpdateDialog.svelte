<script lang="ts">
  import { formatBytes, updates, whatsNew } from "$lib/updates.svelte";

  interface Props {
    /** Runs before installing: saves or confirms unsaved work. False cancels the install. */
    beforeInstall: () => Promise<boolean>;
  }
  let { beforeInstall }: Props = $props();

  let dialog = $state<HTMLDialogElement | undefined>();

  const u = $derived(updates.update);
  const notes = $derived(whatsNew(u?.body));
  const busy = $derived(updates.status.kind === "downloading" || updates.status.kind === "restarting");
  const percent = $derived(
    updates.status.kind === "downloading" && updates.status.total
      ? Math.min(100, Math.round((updates.status.done / updates.status.total) * 100))
      : null,
  );

  $effect(() => {
    if (!dialog) return;
    if (updates.prompt && !dialog.open) dialog.showModal();
    else if (!updates.prompt && dialog.open) dialog.close();
  });

  function released(date: string | undefined) {
    if (!date) return "";
    const d = new Date(date.replace(" ", "T").replace(/\.\d+ /, " "));
    return isNaN(d.getTime()) ? "" : d.toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" });
  }

  async function install() {
    if (!updates.portable && !(await beforeInstall())) return;
    await updates.install();
  }

  /** Escape or a click outside counts as "later", except while installing. */
  function cancel(e: Event) {
    e.preventDefault();
    if (!busy) updates.later();
  }
</script>

<dialog
  bind:this={dialog}
  class="update"
  aria-labelledby="update-title"
  oncancel={cancel}
  onclick={(e) => e.target === dialog && cancel(e)}
>
  {#if u}
    <div class="body">
      <div class="head">
        <span class="badge" aria-hidden="true">↑</span>
        <div>
          <h2 id="update-title">A new version of ProCalc is available</h2>
          <p class="sub">
            Version <strong>{u.version}</strong>{#if released(u.date)} · released {released(u.date)}{/if}
            · you have {updates.version}
          </p>
        </div>
      </div>

      {#if notes}
        <div class="notes-label">What's new</div>
        <div class="notes">{notes}</div>
      {/if}
      <button class="link" onclick={() => updates.openReleasePage()}>Full release notes ↗</button>

      {#if updates.portable}
        <p class="hint">
          This is the portable version, so it can't update itself. Download the new zip and replace
          this folder, or switch to the installer to get automatic updates.
        </p>
      {/if}

      {#if updates.status.kind === "downloading"}
        <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent ?? undefined}>
          <div class="bar" class:indeterminate={percent === null} style:width={percent === null ? undefined : `${percent}%`}></div>
        </div>
        <p class="hint">
          Downloading… {formatBytes(updates.status.done)}{#if updates.status.total} of {formatBytes(updates.status.total)}{/if}
        </p>
      {:else if updates.status.kind === "restarting"}
        <p class="hint">Installed. Restarting ProCalc…</p>
      {:else if updates.status.kind === "error"}
        <p class="error">The update didn't install: {updates.status.message}</p>
      {/if}

      <div class="buttons">
        <button class="ghost" disabled={busy} onclick={() => updates.skip()}>Skip this version</button>
        <span class="spacer"></span>
        <button class="ghost" disabled={busy} onclick={() => updates.later()}>Remind me later</button>
        <button class="primary" disabled={busy} onclick={install}>
          {#if updates.portable}Download{:else if updates.status.kind === "error"}Try again{:else}Install now{/if}
        </button>
      </div>
      {#if !updates.portable && !busy}
        <p class="fine">ProCalc will restart to finish. Your projects and history are kept.</p>
      {/if}
    </div>
  {/if}
</dialog>

<style>
  .update {
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--ink);
    padding: 0;
    width: min(520px, calc(100vw - 32px));
    max-height: calc(100dvh - 48px);
  }

  .update::backdrop {
    background: rgb(0 0 0 / 0.45);
  }

  .body {
    padding: 18px 20px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .head {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }

  .badge {
    flex: none;
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 700;
    font-size: 18px;
  }

  h2 {
    margin: 0;
    font-size: 16.5px;
  }

  .sub {
    margin: 2px 0 0;
    color: var(--muted);
    font-size: 13px;
  }

  .notes-label {
    font-weight: 600;
    font-size: 13px;
  }

  .notes {
    white-space: pre-wrap;
    max-height: 220px;
    overflow-y: auto;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--bg);
    font-size: 13px;
    line-height: 1.55;
  }

  .link {
    align-self: flex-start;
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }

  .link:hover {
    text-decoration: underline;
  }

  .hint,
  .fine,
  .error {
    margin: 0;
    font-size: 12.5px;
    color: var(--muted);
  }

  .error {
    color: var(--err);
  }

  .fine {
    text-align: right;
  }

  .progress {
    height: 6px;
    border-radius: 3px;
    background: var(--line);
    overflow: hidden;
  }

  .bar {
    height: 100%;
    background: var(--accent);
    transition: width 120ms linear;
  }

  .bar.indeterminate {
    width: 35%;
    animation: slide 1.1s ease-in-out infinite;
  }

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(300%);
    }
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }

  .spacer {
    flex: 1;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .ghost {
    background: none;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px 12px;
    font: inherit;
    font-size: 13px;
    color: var(--muted);
    cursor: pointer;
  }

  .ghost:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--muted);
  }

  .primary {
    border: none;
    border-radius: 8px;
    padding: 7px 16px;
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    background: var(--accent);
    color: var(--accent-ink);
    cursor: pointer;
  }

  .primary:hover:not(:disabled) {
    filter: brightness(1.08);
  }
</style>
