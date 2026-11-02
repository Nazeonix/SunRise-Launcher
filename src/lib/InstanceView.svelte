<script>
  import { app, play, stop, updateInstance, deleteInstance, toast } from './state.svelte.js';
  import { api } from './api.js';
  import Banner from './Banner.svelte';
  import WavyProgress from './WavyProgress.svelte';
  import Icon from './Icon.svelte';
  import InstanceForm from './InstanceForm.svelte';
  import { cookie } from './shapes.js';
  import { convertFileSrc } from '@tauri-apps/api/core';

  let { instance } = $props();

  const LOADERS = { vanilla: 'Vanilla', fabric: 'Fabric', forge: 'Forge', neoforge: 'NeoForge' };
  const TABS = [
    { id: 'overview', label: 'Overview', icon: 'info' },
    { id: 'mods', label: 'Mods', icon: 'puzzle' },
    { id: 'resourcepacks', label: 'Resource Packs', icon: 'sparkle' },
    { id: 'logs', label: 'Logs', icon: 'doc' },
    { id: 'settings', label: 'Settings', icon: 'gear' },
  ];

  const game = $derived(app.game[instance.id] ?? { state: 'idle', message: '', needJava: null });
  const prog = $derived(app.progress[instance.id] ?? { text: '', fraction: 0 });
  const installing = $derived(game.state === 'installing');
  const blocked = $derived(app.blocked[instance.id] ?? []);
  const running = $derived(game.state === 'running');
  const kind = $derived(app.versions.find((v) => v.id === instance.mcVersion)?.type === 'snapshot' ? 'Snapshot' : 'Release');
  const loaderName = $derived(LOADERS[instance.loader] ?? instance.loader);
  const subtitle = $derived(
    instance.loader === 'vanilla' ? `Minecraft ${instance.mcVersion}` : `Minecraft ${instance.mcVersion} · ${loaderName}${instance.loaderVersion ? ' ' + instance.loaderVersion : ''}`
  );

  let tab = $state('overview');

  // ── logs: live tail while the tab is open
  let log = $state('');
  let logBox = $state();
  $effect(() => {
    if (tab !== 'logs') return;
    let alive = true;
    const load = async () => {
      try {
        const t = await api.readLog(instance.id);
        if (alive) {
          log = t;
          queueMicrotask(() => logBox && (logBox.scrollTop = logBox.scrollHeight));
        }
      } catch {}
    };
    load();
    const h = setInterval(load, 1500);
    return () => { alive = false; clearInterval(h); };
  });

  // ── mods
  let mods = $state([]);
  let modQuery = $state('');
  let modResults = $state([]);
  let searchedMods = $state(false);
  let searchingMods = $state(false);
  let addingMod = $state('');
  async function loadMods() {
    try { mods = await api.mods(instance.id); } catch (e) { toast(e); }
  }
  $effect(() => { if (tab === 'mods') loadMods(); });
  async function toggle(m) { try { await api.toggleMod(instance.id, m.name); } catch (e) { toast(e); } loadMods(); }
  async function remove(m) { try { await api.deleteMod(instance.id, m.name); } catch (e) { toast(e); } loadMods(); }
  async function searchMods() {
    searchingMods = true;
    try {
      modResults = await api.searchMods(instance.id, modQuery);
      searchedMods = true;
    } catch (e) { toast(e); }
    searchingMods = false;
  }
  async function addMod(mod) {
    addingMod = mod.id;
    try {
      const files = await api.installMod(instance.id, mod.id);
      await loadMods();
      toast(files.length ? `Installed ${files.length} mod file${files.length === 1 ? '' : 's'} (including required dependencies)` : 'This mod and its dependencies are already installed');
    } catch (e) { toast(e); }
    addingMod = '';
  }
  const fmtSize = (n) => (n > 1048576 ? (n / 1048576).toFixed(1) + ' MB' : Math.max(1, Math.round(n / 1024)) + ' KB');
  const fmtDownloads = (n) => n >= 1000000 ? `${(n / 1000000).toFixed(1)}M` : n >= 1000 ? `${(n / 1000).toFixed(1)}K` : String(n);

  let resourcePacks = $state([]);
  let resourcePackQuery = $state('');
  let resourcePackResults = $state([]);
  let searchedResourcePacks = $state(false);
  let searchingResourcePacks = $state(false);
  let addingResourcePack = $state('');
  async function loadResourcePacks() {
    try { resourcePacks = await api.resourcePacks(instance.id); } catch (e) { toast(e); }
  }
  $effect(() => { if (tab === 'resourcepacks') loadResourcePacks(); });
  async function searchResourcePacks() {
    searchingResourcePacks = true;
    try {
      resourcePackResults = await api.searchResourcePacks(instance.id, resourcePackQuery);
      searchedResourcePacks = true;
    } catch (e) { toast(e); }
    searchingResourcePacks = false;
  }
  async function addResourcePack(pack) {
    addingResourcePack = pack.id;
    try {
      const name = await api.installResourcePack(instance.id, pack.id);
      await loadResourcePacks();
      toast(`Installed ${name} to this instance`);
    } catch (e) { toast(e); }
    addingResourcePack = '';
  }
  async function removeResourcePack(pack) {
    try {
      await api.deleteResourcePack(instance.id, pack.name);
      await loadResourcePacks();
      toast(`Removed ${pack.name}`);
    } catch (e) { toast(e); }
  }

  // ── settings tab
  // intentional: the form starts as a copy of the instance (this view is re-created per instance)
  // svelte-ignore state_referenced_locally
  let form = $state({ ...$state.snapshot(instance) });
  let saving = $state(false);
  let confirmDel = $state(false);
  let delFiles = $state(false);
  async function save() {
    saving = true;
    try {
      await updateInstance({ ...$state.snapshot(form), name: form.name.trim() || instance.name });
      toast('Saved');
    } catch (e) { toast(e); }
    saving = false;
  }
  async function del() {
    try { await deleteInstance(instance.id, delFiles); } catch (e) { toast(e); }
  }

  const fmtDate = (ts) => (ts ? new Date(ts * 1000).toLocaleString() : 'Never');
  const javaLabel = $derived(instance.javaPath || app.settings?.javaPath || 'Auto');
  const ramLabel = $derived(instance.ramMb > 0 ? `${instance.ramMb} MB` : `${app.settings?.ramMb ?? 4096} MB (default)`);
  const sep = $derived(app.info.root.includes(String.fromCharCode(92)) ? String.fromCharCode(92) : '/');
  const gameDir = $derived(instance.dir || `${app.info.instancesDir}${sep}${instance.id}`);
</script>

<div class="view">
  <header>
    {#if instance.iconUrl}<img class="ico custom" src={/^(https?:|data:|asset:)/i.test(instance.iconUrl) ? instance.iconUrl : convertFileSrc(instance.iconUrl)} alt="" />{:else}<div class="ico" style:background={instance.color} style:clip-path={cookie(8, 0.07)}>{instance.name.slice(0, 1).toUpperCase()}</div>{/if}
    <div class="t">
      <h1>{instance.name}</h1>
      <p class="hint">Last played: {fmtDate(instance.lastPlayed)}</p>
    </div>
  </header>

  <Banner title={instance.name} {subtitle} imageUrl={instance.bannerUrl} />

  <div class="actions">
    {#if running}
      <button class="btn play stop" onclick={() => stop(instance.id)}><Icon name="stop" size={22} /> Stop</button>
    {:else}
      <button class="btn primary play" disabled={installing} onclick={() => play(instance.id)}>
        {#if installing}<span class="spin"></span> Installing…{:else}<Icon name="play" size={22} /> Play{/if}
      </button>
    {/if}

    <div class="card ver">
      <div class="cube" style:clip-path={cookie(8, 0.07)}>
        <svg width="26" height="26" viewBox="0 0 26 26" aria-hidden="true">
          <path d="M13 4l8 4.5v9L13 22l-8-4.5v-9z" fill="#8A5A3C" /><path d="M13 4l8 4.5L13 13 5 8.5z" fill="#66BB6A" /><path d="M13 13v9l8-4.5v-9z" fill="#6E4630" />
        </svg>
      </div>
      <div class="vt"><small>{kind}</small><b>{instance.mcVersion}</b></div>
      {#if instance.loader !== 'vanilla'}<span class="pill tert">{loaderName}</span>{/if}
    </div>

    <button class="card nav" onclick={() => api.openFolder(instance.id, 'game').catch(toast)}>
      <Icon name="folder" /> <span>Folder</span>
    </button>
  </div>

  {#if installing}
    <div class="status">
      <div class="st-text"><span class="spin"></span> {prog.text}</div>
      <WavyProgress fraction={prog.fraction} animate />
    </div>
  {:else if running}
    <div class="status ok"><span class="dot"></span> Minecraft is running</div>
  {/if}

  {#if game.state === 'error'}
    <div class="card errcard">
      <p class="err">{game.message}</p>
      {#if game.needJava}
        <button class="btn sm primary" onclick={() => api.openUrl(`https://adoptium.net/temurin/releases/?version=${game.needJava}`)}>Download Java {game.needJava} (adoptium.net)</button>
      {/if}
    </div>
  {/if}

  {#if blocked.length}
    <div class="card blockedcard">
      <div class="bh">
        <b>{blocked.length} file{blocked.length === 1 ? '' : 's'} must be downloaded manually</b>
        <button class="btn sm" onclick={() => (app.blocked[instance.id] = [])}>Dismiss</button>
      </div>
      <p class="hint">Their authors disabled third-party downloads on CurseForge. Open each page, download the file, and drop it into the mods folder.</p>
      <div class="blist">
        {#each blocked as b (b.url)}
          <div class="brow"><span>{b.name}</span><button class="btn sm" onclick={() => api.openUrl(b.url).catch(toast)}>Open page</button></div>
        {/each}
      </div>
      <button class="btn sm" onclick={() => api.openFolder(instance.id, 'mods').catch(toast)}><Icon name="folder" size={18} /> Open mods folder</button>
    </div>
  {/if}

  <div class="tabs" role="tablist" aria-label="Instance sections">
    {#each TABS as t (t.id)}
      <button class="tab" class:on={tab === t.id} onclick={() => (tab = t.id)} role="tab" aria-selected={tab === t.id} aria-label={t.label}>
        <span class="tab-icon"><Icon name={t.icon} size={19} /></span><span class="tab-label">{t.label}</span>
      </button>
    {/each}
  </div>

  <section>
    {#if tab === 'overview'}
      <div class="grid card">
        <div><small>Minecraft</small><b>{instance.mcVersion} <span class="pill">{kind}</span></b></div>
        <div><small>Mod loader</small><b>{loaderName}{instance.loaderVersion ? ' ' + instance.loaderVersion : instance.loader !== 'vanilla' ? ' (recommended)' : ''}</b></div>
        <div><small>Java</small><b class="wrap">{javaLabel}</b></div>
        <div><small>Memory</small><b>{ramLabel}</b></div>
        <div class="full"><small>Game folder</small><b class="wrap">{gameDir}</b></div>
      </div>
      <div class="row" style="margin-top:12px">
        <button class="btn sm" onclick={() => api.openFolder(instance.id, 'game').catch(toast)}><Icon name="folder" size={18} /> Open game folder</button>
        <button class="btn sm" onclick={() => api.openFolder(instance.id, 'mods').catch(toast)}><Icon name="puzzle" size={18} /> Open mods folder</button>
      </div>
    {:else if tab === 'mods'}
      {#if instance.loader === 'vanilla'}
        <p class="hint">Vanilla has no mods. Switch the mod loader to Fabric, Forge or NeoForge in this instance's <b>Settings</b> tab, then drop mod <code>.jar</code> files into the mods folder.</p>
      {:else}
        <div class="modrinth-search card">
          <div class="modrinth-head">
            <div><b>Add mods from Modrinth</b><p class="hint">Showing mods compatible with Minecraft {instance.mcVersion} and {loaderName}.</p></div>
            <span class="mr-badge">MODRINTH</span>
          </div>
          <form class="search-form" onsubmit={(e) => { e.preventDefault(); searchMods(); }}>
            <input class="search-input" type="search" bind:value={modQuery} placeholder="Search mods, or leave empty for popular mods" aria-label="Search Modrinth mods" />
            <button class="btn primary sm" type="submit" disabled={searchingMods}>{searchingMods ? 'Searching…' : 'Search'}</button>
          </form>
          {#if searchingMods}
            <p class="hint search-message">Searching Modrinth…</p>
          {:else if searchedMods && !modResults.length}
            <p class="hint search-message">No compatible mods found. Try another search.</p>
          {:else if modResults.length}
            <div class="search-results">
              {#each modResults as mod (mod.id)}
                <article class="search-result">
                  {#if mod.iconUrl}<img src={mod.iconUrl} alt="" class="mod-icon" />{:else}<div class="mod-icon missing"><Icon name="puzzle" size={19} /></div>{/if}
                  <div class="mod-info">
                    <b>{mod.title}</b>
                    <span class="hint">by {mod.author} · {fmtDownloads(mod.downloads)} downloads</span>
                    <p>{mod.description}</p>
                  </div>
                  <div class="mod-actions">
                    <button class="btn sm" onclick={() => api.openUrl(mod.pageUrl).catch(toast)}>View</button>
                    <button class="btn primary sm" disabled={!!addingMod} onclick={() => addMod(mod)}>{addingMod === mod.id ? 'Adding…' : 'Add'}</button>
                  </div>
                </article>
              {/each}
            </div>
          {/if}
        </div>
        <div class="row" style="margin-bottom:10px">
          <button class="btn sm" onclick={() => api.openFolder(instance.id, 'mods').catch(toast)}><Icon name="folder" size={18} /> Open mods folder</button>
          <button class="btn sm" onclick={loadMods}>Refresh</button>
          <span class="hint">{mods.length} mod{mods.length === 1 ? '' : 's'}</span>
        </div>
        {#if instance.loader === 'fabric'}<p class="hint" style="margin-bottom:10px">Most Fabric mods also need the <b>Fabric API</b> mod.</p>{/if}
        <div class="mods card">
          {#each mods as m (m.name)}
            <div class="mod" class:off={!m.enabled}>
              <span class="switch"><input type="checkbox" checked={m.enabled} onchange={() => toggle(m)} aria-label="Enable {m.name}" /><span></span></span>
              <span class="mn">{m.name.replace(/\.disabled$/, '')}</span>
              <small>{fmtSize(m.size)}</small>
              <button class="icon-btn" aria-label="Delete {m.name}" onclick={() => remove(m)}><Icon name="trash" size={18} /></button>
            </div>
          {:else}
            <p class="hint pad">No mods yet – put <code>.jar</code> files in the mods folder.</p>
          {/each}
        </div>
      {/if}
    {:else if tab === 'resourcepacks'}
      <div class="modrinth-search card">
        <div class="modrinth-head">
          <div><b>Download resource packs from Modrinth</b><p class="hint">Showing packs compatible with Minecraft {instance.mcVersion}. Installed into this instance only.</p></div>
          <span class="mr-badge">MODRINTH</span>
        </div>
        <form class="search-form" onsubmit={(e) => { e.preventDefault(); searchResourcePacks(); }}>
          <input class="search-input" type="search" bind:value={resourcePackQuery} placeholder="Search resource packs, or leave empty for popular packs" aria-label="Search Modrinth resource packs" />
          <button class="btn primary sm" type="submit" disabled={searchingResourcePacks}>{searchingResourcePacks ? 'Searching…' : 'Search'}</button>
        </form>
        {#if searchingResourcePacks}
          <p class="hint search-message">Searching Modrinth…</p>
        {:else if searchedResourcePacks && !resourcePackResults.length}
          <p class="hint search-message">No compatible resource packs found. Try another search.</p>
        {:else if resourcePackResults.length}
          <div class="search-results">
            {#each resourcePackResults as pack (pack.id)}
              <article class="search-result">
                {#if pack.iconUrl}<img src={pack.iconUrl} alt="" class="mod-icon" />{:else}<div class="mod-icon missing"><Icon name="sparkle" size={19} /></div>{/if}
                <div class="mod-info">
                  <b>{pack.title}</b>
                  <span class="hint">by {pack.author} · {fmtDownloads(pack.downloads)} downloads</span>
                  <p>{pack.description}</p>
                </div>
                <div class="mod-actions">
                  <button class="btn sm" onclick={() => api.openUrl(pack.pageUrl).catch(toast)}>View</button>
                  <button class="btn primary sm" disabled={!!addingResourcePack} onclick={() => addResourcePack(pack)}>{addingResourcePack === pack.id ? 'Adding…' : 'Add'}</button>
                </div>
              </article>
            {/each}
          </div>
        {/if}
      </div>
      <div class="row" style="margin-bottom:10px">
        <button class="btn sm" onclick={() => api.openFolder(instance.id, 'resourcepacks').catch(toast)}><Icon name="folder" size={18} /> Open resource packs folder</button>
        <button class="btn sm" onclick={loadResourcePacks}>Refresh</button>
        <span class="hint">{resourcePacks.length} resource pack{resourcePacks.length === 1 ? '' : 's'}</span>
      </div>
      <div class="mods card">
        {#each resourcePacks as pack (pack.name)}
          <div class="mod">
            <Icon name="sparkle" size={19} />
            <span class="mn">{pack.name}</span>
            <small>{fmtSize(pack.size)}</small>
            <button class="icon-btn" aria-label="Remove {pack.name}" onclick={() => removeResourcePack(pack)}><Icon name="trash" size={18} /></button>
          </div>
        {:else}
          <p class="hint pad">No resource packs installed yet.</p>
        {/each}
      </div>
    {:else if tab === 'logs'}
      <pre class="log card" bind:this={logBox}>{log || 'No log yet – press Play.'}</pre>
    {:else}
      <div class="card pad">
        <InstanceForm bind:form />
        <div class="foot"><button class="btn primary" disabled={saving} onclick={save}>Save changes</button></div>
      </div>

      <div class="card pad danger-zone">
        {#if !confirmDel}
          <button class="btn sm danger" onclick={() => (confirmDel = true)}><Icon name="trash" size={18} /> Delete instance</button>
        {:else}
          <p><b>Delete “{instance.name}”?</b> This can't be undone.</p>
          <label class="row" style="margin:10px 0"><span class="switch"><input type="checkbox" bind:checked={delFiles} /><span></span></span> Also delete worlds, mods and settings</label>
          <div class="row">
            <button class="btn sm danger" onclick={del}>Delete</button>
            <button class="btn sm" onclick={() => (confirmDel = false)}>Cancel</button>
          </div>
        {/if}
      </div>
    {/if}
  </section>
</div>

<style>
  .view { max-width: 860px; margin: 0 auto; display: grid; gap: 16px; }
  header { display: flex; align-items: center; gap: 14px; }
  .ico { width: 52px; height: 52px; display: grid; place-items: center; color: #fff; font-weight: 800; font-size: 22px; flex: none; object-fit: cover; border-radius: 17px 13px 18px 13px; }
  .ico.custom { object-fit: cover; }
  h1 { font-size: 28px; line-height: 1.1; }

  .actions { display: grid; grid-template-columns: 1.1fr 1.2fr .8fr; gap: 12px; }
  .play { height: 60px; font-size: 22px; border-radius: 14px; gap: 10px; }
  .play.stop { background: var(--tertiary-container); color: var(--on-tertiary-container); }
  .card.ver { display: flex; align-items: center; gap: 12px; padding: 0 16px; height: 60px; }
  .cube { width: 40px; height: 40px; background: var(--primary-container); display: grid; place-items: center; flex: none; }
  .vt { display: grid; line-height: 1.15; flex: 1; min-width: 0; }
  .vt small { color: var(--text-dim); font-weight: 700; font-size: 12px; }
  .vt b { font-size: 20px; }
  .card.nav { display: flex; align-items: center; justify-content: center; gap: 10px; height: 60px; font-weight: 700; transition: background .15s, border-radius .3s var(--spring); }
  .card.nav:hover { background: var(--secondary-container); }
  .card.nav:active { border-radius: 6px; }

  .status { display: grid; gap: 8px; }
  .st-text { display: flex; align-items: center; gap: 10px; justify-content: center; font-weight: 700; }
  .status.ok { display: flex; align-items: center; justify-content: center; gap: 10px; font-weight: 700; color: var(--primary); }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: #4CAF50; box-shadow: 0 0 0 4px color-mix(in srgb, #4CAF50 25%, transparent); }
  .errcard { padding: 12px 16px; display: flex; align-items: center; gap: 14px; justify-content: space-between; flex-wrap: wrap; }

  /* expressive navigation: selected destinations grow into a filled pill */
  .tabs { justify-self: center; display: inline-flex; gap: 4px; padding: 7px; background: var(--surface-high); border: 1px solid var(--outline); border-radius: 22px 18px 24px 18px; }
  .tab {
    height: 42px; min-width: 46px; padding: 0 12px; display: inline-flex; align-items: center; justify-content: center; gap: 8px; border-radius: 15px; color: var(--on-secondary-container);
    transition: background .2s, color .2s, border-radius .35s var(--spring), transform .25s var(--spring), padding .25s var(--spring);
  }
  .tab:hover { background: color-mix(in srgb, var(--on-secondary-container) 9%, transparent); transform: translateY(-1px); }
  .tab-icon { width: 28px; height: 28px; display: grid; place-items: center; border-radius: 10px; transition: background .2s, transform .3s var(--spring); }
  .tab-label { font-size: 13px; font-weight: 650; white-space: nowrap; }
  .tab.on { background: var(--primary); color: var(--on-primary); padding: 0 15px; border-radius: 16px 21px 17px 19px; box-shadow: 0 3px 9px color-mix(in srgb, var(--primary) 25%, transparent); }
  .tab.on .tab-icon { background: color-mix(in srgb, var(--on-primary) 16%, transparent); transform: rotate(-5deg) scale(1.04); }
  .tab:active { transform: scale(.96); }
  @media (max-width: 1100px) {
    .tabs { gap: 2px; padding: 5px; }
    .tab { gap: 5px; min-width: 0; padding: 0 8px; }
    .tab-label { font-size: 12px; }
    .tab.on { padding: 0 10px; }
  }

  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 1px; background: var(--outline); overflow: hidden; }
  .grid > div { background: var(--surface); padding: 14px 18px; display: grid; gap: 4px; }
  .grid .full { grid-column: 1 / -1; }
  .grid small { color: var(--text-dim); font-weight: 700; font-size: 12px; text-transform: uppercase; letter-spacing: .5px; }
  .grid b { display: flex; align-items: center; gap: 8px; font-size: 16px; }
  .grid b.wrap { word-break: break-all; font-size: 14px; display: block; }

  code { background: var(--surface-high); padding: 1px 6px; border-radius: 6px; font-size: 12.5px; }
  .pad { padding: 4px 20px 20px; }
  .mods { overflow: hidden; }
  .mod { display: flex; align-items: center; gap: 12px; padding: 8px 12px 8px 14px; border-bottom: 1px solid var(--outline); }
  .mod:last-child { border-bottom: 0; }
  .mod.off .mn { opacity: .5; text-decoration: line-through; }
  .mn { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
  .mod small { color: var(--text-dim); }

  .modrinth-search { padding: 16px; margin-bottom: 12px; }
  .modrinth-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .modrinth-head p { margin: 4px 0 0; }
  .mr-badge { flex: none; font-size: 10px; font-weight: 800; letter-spacing: .8px; color: var(--primary); background: var(--primary-container); padding: 6px 9px; border-radius: 999px; }
  .search-form { display: flex; gap: 8px; margin-top: 14px; }
  .search-input { flex: 1; min-width: 0; padding: 10px 13px; color: var(--text); background: var(--surface-high); border: 1px solid var(--outline); border-radius: 12px; outline: none; }
  .search-input:focus { border-color: var(--primary); }
  .search-message { margin: 14px 0 0; }
  .search-results { display: grid; margin-top: 10px; max-height: 410px; overflow-y: auto; }
  .search-result { display: flex; align-items: center; gap: 12px; padding: 12px 0; border-top: 1px solid var(--outline); }
  .mod-icon { width: 42px; height: 42px; border-radius: 11px; object-fit: cover; flex: none; background: var(--surface-high); }
  .mod-icon.missing { display: grid; place-items: center; color: var(--text-dim); }
  .mod-info { min-width: 0; flex: 1; display: grid; gap: 3px; }
  .mod-info b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mod-info p { margin: 1px 0 0; color: var(--text-dim); font-size: 12px; line-height: 1.35; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .mod-info .hint { font-size: 11px; }
  .mod-actions { display: flex; gap: 6px; flex: none; }

  .log { margin: 0; padding: 14px 16px; height: 300px; overflow: auto; font: 12px/1.5 ui-monospace, 'JetBrains Mono', Menlo, monospace; color: var(--text-dim); white-space: pre-wrap; word-break: break-all; }
  .blockedcard { padding: 14px 16px; display: grid; gap: 10px; }
  .bh { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .blist { display: grid; gap: 6px; max-height: 220px; overflow-y: auto; }
  .brow { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 6px 10px; border-radius: 10px; background: var(--surface-high); }
  .brow span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
  .danger-zone { margin-top: 14px; padding-top: 18px; }
  @media (max-width: 600px) { .search-result { align-items: flex-start; } .mod-actions { flex-direction: column; } .mr-badge { display: none; } }
</style>
