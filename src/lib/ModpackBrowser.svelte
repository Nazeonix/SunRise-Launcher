<script>
  import Modal from './Modal.svelte';
  import Seg from './Seg.svelte';
  import { app, refreshInstances, toast } from './state.svelte.js';
  import { api } from './api.js';

  const SOURCES = [
    { value: 'modrinth', label: 'Modrinth' },
    // CurseForge is intentionally disabled; its API integration requires a dedicated key.
    // { value: 'curseforge', label: 'CurseForge' },
  ];

  let source = $state('modrinth');
  let query = $state('');
  let results = $state([]);
  let loading = $state(false);
  let error = $state('');
  let more = $state(false);
  let reqId = 0;

  let selected = $state(null);
  let versions = $state([]);
  let versionId = $state('');
  let vloading = $state(false);
  let verr = $state('');
  let name = $state('');
  let installing = $state(false);

  const fmt = (n) => (n >= 1e6 ? (n / 1e6).toFixed(1) + 'M' : n >= 1e3 ? Math.round(n / 1e3) + 'K' : String(n));
  const label = (v) => {
    const mc = v.mcVersions.slice(0, 3).join(', ');
    const ld = v.loaders.join('/');
    return [v.name, [mc, ld].filter(Boolean).join(' · '), v.date].filter(Boolean).join('  —  ');
  };

  async function search(reset = true) {
    const id = ++reqId;
    loading = true;
    error = '';
    try {
      const offset = reset ? 0 : results.length;
      const r = await api.searchPacks(source, query.trim(), offset);
      if (id !== reqId) return;
      results = reset ? r : [...results, ...r];
      more = r.length >= 20;
    } catch (e) {
      if (id !== reqId) return;
      error = String(e);
      if (reset) results = [];
      more = false;
    }
    if (id === reqId) loading = false;
  }

  // search as you type (debounced); also re-runs when the source changes
  let timer;
  $effect(() => {
    source; query;
    selected = null;
    clearTimeout(timer);
    timer = setTimeout(() => search(true), 300);
    return () => clearTimeout(timer);
  });

  async function pick(r) {
    selected = r;
    versions = [];
    versionId = '';
    verr = '';
    name = r.title;
    vloading = true;
    try {
      const v = await api.packVersions(source, r.id);
      if (selected?.id !== r.id) return;
      versions = v;
      versionId = v[0]?.id ?? '';
      if (!v.length) verr = 'This pack has no downloadable versions.';
    } catch (e) {
      verr = String(e);
    }
    vloading = false;
  }

  function opened(inst) {
    app.game[inst.id] = { state: 'installing', message: '', needJava: null };
    app.progress[inst.id] = { text: 'Starting…', fraction: 0 };
    app.selectedId = inst.id;
    app.dialog = null;
  }

  async function install() {
    if (!selected || !versionId) return;
    installing = true;
    try {
      const inst = await api.installPack({ source, projectId: selected.id, versionId, name: name.trim() || selected.title });
      await refreshInstances();
      opened(inst);
    } catch (e) {
      toast(e);
    }
    installing = false;
  }

  async function importFile() {
    try {
      const p = await api.pickModpackFile();
      if (!p) return;
      const inst = await api.importPack(p, '');
      await refreshInstances();
      opened(inst);
    } catch (e) {
      toast(e);
    }
  }
</script>

<Modal title="Modpacks" wide onclose={() => (app.dialog = null)}>
  {#if !selected}
    <div class="bar">
      <Seg options={SOURCES} bind:value={source} />
      <input class="input" placeholder="Search modpacks…" bind:value={query} />
      <button class="btn sm" onclick={importFile} title="Install a Modrinth .mrpack from your computer">Import file…</button>
    </div>

    {#if error}
      <div class="notice">
        <p class="err">{error}</p>
        <!-- CurseForge API key controls are disabled with the CurseForge integration.
        {#if source === 'curseforge'}
          <div class="row">
            <button class="btn sm primary" onclick={() => (app.dialog = 'settings')}>Open Settings</button>
            <button class="btn sm" onclick={() => api.openUrl('https://console.curseforge.com/').catch(toast)}>Get a free API key</button>
          </div>
        {/if}
        -->
      </div>
    {/if}

    <div class="results">
      {#each results as r (r.source + r.id)}
        <button class="pack" onclick={() => pick(r)}>
          {#if r.iconUrl}<img src={r.iconUrl} alt="" loading="lazy" onerror={(e) => (e.currentTarget.style.visibility = 'hidden')} />{:else}<span class="img"></span>{/if}
          <span class="info">
            <b>{r.title}</b>
            <small>by {r.author || 'unknown'} · {fmt(r.downloads)} downloads</small>
            <span class="desc">{r.description}</span>
          </span>
        </button>
      {:else}
        {#if !loading && !error}<p class="hint pad">No modpacks found.</p>{/if}
      {/each}
    </div>

    <div class="more">
      {#if loading}<span class="spin"></span>{:else if more}<button class="btn sm" onclick={() => search(false)}>Load more</button>{/if}
    </div>
  {:else}
    <button class="btn sm" onclick={() => (selected = null)}>← Back</button>
    <div class="detail">
      {#if selected.iconUrl}<img src={selected.iconUrl} alt="" />{:else}<span class="img big"></span>{/if}
      <div>
        <h3>{selected.title}</h3>
        <small class="hint">by {selected.author || 'unknown'} · {fmt(selected.downloads)} downloads</small>
        <p>{selected.description}</p>
        {#if selected.pageUrl}<button class="link" onclick={() => api.openUrl(selected.pageUrl).catch(toast)}>Open page ↗</button>{/if}
      </div>
    </div>

    <div class="field">
      <label for="mp-ver">Version</label>
      {#if vloading}
        <div class="row"><span class="spin"></span> Loading versions…</div>
      {:else}
        <select id="mp-ver" class="select" bind:value={versionId}>
          {#each versions as v (v.id)}<option value={v.id}>{label(v)}</option>{/each}
        </select>
      {/if}
      {#if verr}<p class="err">{verr}</p>{/if}
    </div>

    <div class="field">
      <label for="mp-name">Instance name</label>
      <input id="mp-name" class="input" bind:value={name} />
    </div>

    <p class="hint" style="margin-top:12px">
      Creates a new instance, downloads the pack's mods and files, and sets the right Minecraft version and mod loader.
      Press Play afterwards to download the game itself.
    </p>

    <div class="foot">
      <button class="btn" onclick={() => (app.dialog = null)}>Cancel</button>
      <button class="btn primary" disabled={installing || !versionId} onclick={install}>{installing ? 'Starting…' : 'Install'}</button>
    </div>
  {/if}
</Modal>

<style>
  .bar { display: flex; gap: 10px; align-items: center; margin-bottom: 12px; flex-wrap: wrap; }
  .bar .input { flex: 1; min-width: 160px; }
  .notice { padding: 12px 14px; border: 1px solid var(--outline); border-radius: 12px; background: var(--surface-high); margin-bottom: 10px; display: grid; gap: 10px; }
  .results { display: grid; gap: 8px; max-height: 52vh; overflow-y: auto; padding-right: 4px; }
  .pack { display: flex; gap: 12px; text-align: left; padding: 10px; border-radius: 12px; border: 1px solid var(--outline); background: var(--surface); transition: background .15s, border-radius .3s var(--spring); }
  .pack:hover { background: var(--secondary-container); }
  .pack:active { border-radius: 6px; }
  .pack img, .img { width: 56px; height: 56px; border-radius: 12px; object-fit: cover; flex: none; background: var(--surface-high); }
  .img.big { width: 84px; height: 84px; display: block; }
  .info { display: grid; gap: 2px; min-width: 0; }
  .info small { color: var(--text-dim); }
  .desc { color: var(--text-dim); font-size: 13.5px; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .more { display: grid; place-items: center; min-height: 40px; margin-top: 8px; }
  .pad { padding: 14px; }
  .detail { display: flex; gap: 16px; margin: 14px 0 4px; }
  .detail img { width: 84px; height: 84px; border-radius: 16px; object-fit: cover; flex: none; }
  .detail h3 { font-size: 22px; }
  .detail p { margin-top: 8px; line-height: 1.45; }
  .link { color: var(--primary); font-weight: 700; margin-top: 6px; }
</style>
