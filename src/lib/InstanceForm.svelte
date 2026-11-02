<script>
  import { app } from './state.svelte.js';
  import { api } from './api.js';
  import Seg from './Seg.svelte';
  import { toast } from './state.svelte.js';
  import { convertFileSrc } from '@tauri-apps/api/core';

  let { form = $bindable(), showFolder = false } = $props();

  async function pickFolder() {
    try {
      const p = await api.pickFolder();
      if (p) form.dirParent = p;
    } catch (e) {
      toast(e);
    }
  }

  async function pickImage(field) {
    try {
      const path = await api.pickImage();
      if (path) form[field] = path;
    } catch (e) { toast(e); }
  }
  const imageSrc = (path) => path ? (/^(https?:|data:|asset:)/i.test(path) ? path : convertFileSrc(path)) : '';

  const COLORS = ['#6A52B3', '#2E6B34', '#2B5EA7', '#C2562B', '#B3487A', '#0E8A8A', '#B8860B', '#5A5A66'];
  const LOADERS = [
    { value: 'vanilla', label: 'Vanilla' },
    { value: 'fabric', label: 'Fabric' },
    { value: 'forge', label: 'Forge' },
    { value: 'neoforge', label: 'NeoForge' },
  ];

  let showSnap = $state(app.settings?.showSnapshots ?? false);
  const vlist = $derived(app.versions.filter((v) => showSnap || v.type === 'release' || v.id === form.mcVersion));

  let lvers = $state([]);
  let lerr = $state('');
  let lloading = $state(false);

  // loader builds for the chosen Minecraft version
  $effect(() => {
    const kind = form.loader;
    const mc = form.mcVersion;
    if (kind === 'vanilla' || !mc) {
      lvers = [];
      lerr = '';
      return;
    }
    let cancelled = false;
    lloading = true;
    lerr = '';
    api.loaderVersions(kind, mc)
      .then((v) => {
        if (cancelled) return;
        lvers = v;
        if (form.loaderVersion && !v.includes(form.loaderVersion)) form.loaderVersion = '';
      })
      .catch((e) => {
        if (cancelled) return;
        lvers = [];
        lerr = String(e);
      })
      .finally(() => {
        if (!cancelled) lloading = false;
      });
    return () => { cancelled = true; };
  });
</script>

<div class="field">
  <label for="f-name">Name</label>
  <input id="f-name" class="input" placeholder={`Minecraft ${form.mcVersion}`} bind:value={form.name} />
</div>

<div class="field">
  <label for="f-ver">Minecraft version</label>
  <div class="row">
    <select id="f-ver" class="select" bind:value={form.mcVersion}>
      {#each vlist as v (v.id)}
        <option value={v.id}>{v.id}{v.type === 'snapshot' ? ' (snapshot)' : ''}</option>
      {/each}
    </select>
    <label class="row snap"><span class="switch"><input type="checkbox" bind:checked={showSnap} /><span></span></span> Snapshots</label>
  </div>
</div>

<div class="field">
  <span class="label">Mod loader</span>
  <Seg options={LOADERS} bind:value={form.loader} onchange={() => (form.loaderVersion = '')} />
  {#if form.loader !== 'vanilla'}
    <div class="row">
      <select class="select" bind:value={form.loaderVersion} aria-label="Loader version">
        <option value="">Recommended build{lvers[0] ? ` (${lvers[0]})` : ''}</option>
        {#each lvers as v, i}<option value={v}>{v}{i === 0 ? ' · recommended' : ''}</option>{/each}
      </select>
      {#if lloading}<span class="spin"></span>{/if}
    </div>
    {#if lerr}<p class="err">{lerr}</p>{/if}
  {/if}
</div>

<div class="field">
  <label for="f-java">Java</label>
  <select id="f-java" class="select" bind:value={form.javaPath}>
    <option value="">Default (Settings / auto)</option>
    {#each app.javas as j (j.path)}
      <option value={j.path}>Java {j.major} – {j.path}</option>
    {/each}
  </select>
</div>

<div class="field">
  <label for="f-ram">Memory: {form.ramMb > 0 ? `${form.ramMb} MB` : `Default (${app.settings?.ramMb ?? 4096} MB)`}</label>
  <input id="f-ram" type="range" min="0" max="16384" step="512" bind:value={form.ramMb} />
</div>

{#if showFolder}
  <div class="field">
    <span class="label">Instance folder</span>
    <div class="row">
      <input class="input" readonly value={form.dirParent || app.settings?.instancesDir || app.info.instancesDir} aria-label="Instance folder" />
      <button type="button" class="btn sm" onclick={pickFolder}>Choose…</button>
      {#if form.dirParent}<button type="button" class="btn sm" onclick={() => (form.dirParent = '')}>Reset</button>{/if}
    </div>
    <p class="hint">
      {#if form.dirParent || app.settings?.instancesDir}A subfolder named after the instance is created inside it.{:else}Default location (one numbered folder per instance).{/if}
      Worlds, mods and settings are stored there.
    </p>
  </div>
{/if}

<div class="field">
  <span class="label">Colour</span>
  <div class="colors">
    {#each COLORS as c}
      <button type="button" class="sw" class:on={form.color === c} style:background={c} aria-label={c} onclick={() => (form.color = c)}></button>
    {/each}
  </div>
</div>

<div class="field">
  <span class="label">Instance images</span>
  <div class="image-fields">
    <div class="image-control">
      {#if form.iconUrl}<img src={imageSrc(form.iconUrl)} alt="Instance icon preview" class="icon-preview" />{:else}<div class="icon-preview fallback" style:background={form.color}>{(form.name || 'M').slice(0, 1).toUpperCase()}</div>{/if}
      <div><b>Custom icon</b><p class="hint">PNG, JPG, WebP or GIF</p></div>
      <button type="button" class="btn sm" onclick={() => pickImage('iconUrl')}>Choose…</button>
      {#if form.iconUrl}<button type="button" class="icon-btn" aria-label="Remove custom icon" onclick={() => form.iconUrl = ''}>×</button>{/if}
    </div>
    <div class="image-control">
      {#if form.bannerUrl}<img src={imageSrc(form.bannerUrl)} alt="Instance background preview" class="banner-preview" />{:else}<div class="banner-preview fallback">Sunrise background</div>{/if}
      <div><b>Custom background</b><p class="hint">Shown behind the instance title</p></div>
      <button type="button" class="btn sm" onclick={() => pickImage('bannerUrl')}>Choose…</button>
      {#if form.bannerUrl}<button type="button" class="icon-btn" aria-label="Remove custom background" onclick={() => form.bannerUrl = ''}>×</button>{/if}
    </div>
  </div>
</div>

<style>
  .snap { font-size: 13.5px; white-space: nowrap; color: var(--text-dim); font-weight: 600; }
  .colors { display: flex; gap: 8px; flex-wrap: wrap; }
  .sw { width: 30px; height: 30px; border-radius: 10px; transition: border-radius .3s var(--spring), transform .2s var(--spring); }
  .sw:hover { transform: scale(1.1); }
  .sw.on { border-radius: 50%; box-shadow: 0 0 0 3px var(--surface), 0 0 0 5px var(--primary); }
  .image-fields { display: grid; gap: 8px; }
  .image-control { display: flex; align-items: center; gap: 12px; min-width: 0; padding: 10px; border-radius: 14px; background: var(--surface-high); }
  .image-control > div:nth-child(2) { flex: 1; min-width: 0; }
  .image-control .hint { font-size: 12px; }
  .icon-preview { width: 44px; height: 44px; flex: none; object-fit: cover; border-radius: 13px; }
  .icon-preview.fallback { display: grid; place-items: center; color: white; font-size: 20px; font-weight: 800; }
  .banner-preview { width: 88px; height: 44px; flex: none; object-fit: cover; border-radius: 10px; }
  .banner-preview.fallback { display: grid; place-items: center; color: var(--text-dim); font-size: 11px; }
</style>
