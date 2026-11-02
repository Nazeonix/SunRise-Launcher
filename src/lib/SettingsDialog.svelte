<script>
  import Modal from './Modal.svelte';
  import { app, saveSettings, refreshJava, toast } from './state.svelte.js';
  import { api } from './api.js';
  import { MODES, PALETTES, themeOf } from './themes.js';
  import Seg from './Seg.svelte';
  import { cookie } from './shapes.js';

  const s = $derived(app.settings);
  const MAJORS = [25, 21, 17, 11, 8];
  let javaInput = $state('');
  let clientId = $state(app.settings?.msClientId ?? '');
  let ram = $state(app.settings?.ramMb ?? 4096);
  let cfKey = $state(app.settings?.curseforgeKey ?? '');

  const javaFor = (m) => app.javas.find((j) => j.major === m);

  async function chooseInstancesDir() {
    try {
      const p = await api.pickFolder();
      if (p) await saveSettings({ instancesDir: p });
    } catch (e) {
      toast(e);
    }
  }

  async function addJava() {
    try {
      const j = await api.addJava(javaInput.trim());
      javaInput = '';
      await refreshJava();
      await saveSettings({ javaPath: j.path });
      toast(`Added Java ${j.major}`);
    } catch (e) {
      toast(e);
    }
  }
</script>

<Modal title="Settings" wide onclose={() => (app.dialog = null)}>
  <h3>Appearance</h3>
  <div class="themes">
    {#each PALETTES as palette (palette.key)}
      {@const t = themeOf(palette.key, s.mode, app.systemDark)}
      <button class="theme" class:on={s.theme === t.key} onclick={() => saveSettings({ theme: t.key })} style:background={t.bg} style:color={t.text} style:border-color={s.theme === t.key ? t.primary : t.outline}>
        <span class="sw">
          <i style:background={t.primary}></i><i style:background={t.tertiary}></i><i style:background={t.primaryContainer}></i>
        </span>
        <b>{t.name}</b>
      </button>
    {/each}
  </div>
  <div class="field">
    <span class="label">Appearance mode</span>
    <Seg options={MODES} value={s.mode} onchange={(mode) => saveSettings({ mode })} />
  </div>
  <div class="field">
    <span class="label">Instance layout</span>
    <div class="layouts" role="radiogroup" aria-label="Instance layout">
      {#each [{ key: 'list', label: 'Comfortable list', icon: '☰' }, { key: 'grid', label: 'Compact grid', icon: '▦' }] as layout}
        <button class="layout-choice" class:on={(s.instanceLayout || 'list') === layout.key} role="radio" aria-checked={(s.instanceLayout || 'list') === layout.key} onclick={() => saveSettings({ instanceLayout: layout.key })}>
          <span class="layout-glyph">{layout.icon}</span><b>{layout.label}</b><small>{layout.key === 'list' ? 'Names and versions' : 'More instances at once'}</small>
        </button>
      {/each}
    </div>
  </div>

  <h3>Game defaults</h3>
  <div class="field">
    <label for="s-ram">Memory: {ram} MB</label>
    <input id="s-ram" type="range" min="1024" max="16384" step="512" bind:value={ram} onchange={() => saveSettings({ ramMb: ram })} />
  </div>
  {#if app.gpus.length}
    <div class="field">
      <div class="row between">
        <label for="s-gpu">Use dedicated GPU (PRIME offload)</label>
        <span class="switch"><input id="s-gpu" type="checkbox" checked={s.useDiscreteGpu} onchange={(e) => saveSettings({ useDiscreteGpu: e.currentTarget.checked })} /><span></span></span>
      </div>
      <p class="hint">
        {#each app.gpus as g}• {g.vendor.toUpperCase()} ({g.card}){g.bootVga ? ' – primary display GPU' : ''}<br />{/each}
        {#if app.gpus.length < 2}Single GPU – nothing to offload.{/if}
      </p>
    </div>
  {/if}

  <h3>Storage</h3>
  <div class="field" style="margin-top:6px">
    <label for="s-dir">Instances folder</label>
    <div class="row">
      <input id="s-dir" class="input" readonly value={s.instancesDir || app.info.instancesDir} />
      <button class="btn sm" onclick={chooseInstancesDir}>Choose…</button>
      {#if s.instancesDir}<button class="btn sm" onclick={() => saveSettings({ instancesDir: '' })}>Reset</button>{/if}
    </div>
    <p class="hint">Where <b>new</b> instances (worlds, mods, settings) are stored. Existing instances stay where they are. Shared game files (libraries, assets, versions) stay in the data folder below.</p>
  </div>

  <h3>Java</h3>
  <p class="hint">Default for every instance (an instance can override it). “Auto” picks the best match per Minecraft version.</p>
  <div class="badges">
    {#each MAJORS as m}
      {@const j = javaFor(m)}
      <button
        class="badge" class:have={!!j} class:on={!!j && s.javaPath === j.path}
        style:clip-path={cookie(10, 0.07)}
        onclick={() => (j ? saveSettings({ javaPath: j.path }) : api.openUrl(`https://adoptium.net/temurin/releases/?version=${m}`))}
        title={j ? j.path : 'Not installed – click to download'}
      >
        <b>{m}</b><small>{j ? 'installed' : 'get'}</small>
      </button>
    {/each}
  </div>
  <div class="row" style="margin-top:12px">
    <button class="btn sm" class:primary={!s.javaPath} onclick={() => saveSettings({ javaPath: '' })}>Auto</button>
    <span class="hint" style="word-break:break-all">{s.javaPath || 'Sunrise picks the right Java for each version'}</span>
  </div>
  <div class="row" style="margin-top:10px">
    <input class="input" placeholder="Add a Java manually (path to java or its folder)" bind:value={javaInput} />
    <button class="btn sm" disabled={!javaInput.trim()} onclick={addJava}>Add</button>
    <button class="btn sm" onclick={refreshJava}>Re-scan</button>
  </div>

  <h3>Modpacks</h3>
  <div class="field" style="margin-top:6px">
    <label for="s-cf">CurseForge API key</label>
    <input id="s-cf" class="input" type="password" placeholder="Only needed for CurseForge modpacks" bind:value={cfKey} onchange={() => saveSettings({ curseforgeKey: cfKey.trim() })} />
    <p class="hint">Modrinth needs no key. For CurseForge, create a free key at <button class="link" onclick={() => api.openUrl('https://console.curseforge.com/').catch(toast)}>console.curseforge.com</button>. The env var <code>SUNRISE_CF_KEY</code> also works.</p>
  </div>

  <h3>Microsoft sign-in</h3>
  <div class="field" style="margin-top:6px">
    <label for="s-cid">Azure app (client) ID</label>
    <input id="s-cid" class="input" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" bind:value={clientId} onchange={() => saveSettings({ msClientId: clientId.trim() })} />
    <p class="hint">Needs your own Azure app (redirect URI <code>http://localhost</code>, public client) that Mojang has approved – see the README. The env var <code>SUNRISE_MS_CLIENT_ID</code> also works.</p>
  </div>

  <h3>About</h3>
  <p class="hint">Sunrise {app.info.version}<br />Data folder: <code>{app.info.root}</code></p>
</Modal>

<style>
  h3 { margin: 22px 0 6px; font-size: 14px; text-transform: uppercase; letter-spacing: .6px; color: var(--text-dim); }
  h3:first-of-type { margin-top: 6px; }
  .link { color: var(--primary); font-weight: 700; }
  code { background: var(--surface-high); padding: 1px 6px; border-radius: 6px; font-size: 12.5px; }
  .themes { display: grid; grid-template-columns: repeat(auto-fill, minmax(112px, 1fr)); gap: 10px; margin-top: 8px; }
  .layouts { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
  .layout-choice { display: grid; grid-template-columns: 42px 1fr; align-items: center; column-gap: 10px; text-align: left; padding: 12px; border-radius: 16px; border: 1px solid var(--outline); background: var(--surface-high); transition: border-radius .3s var(--spring), border-color .2s, transform .2s; }
  .layout-choice:hover { transform: translateY(-1px); }
  .layout-choice.on { border: 2px solid var(--primary); background: var(--primary-container); color: var(--on-primary-container); border-radius: 19px 15px 20px 15px; }
  .layout-choice b, .layout-choice small { grid-column: 2; }
  .layout-choice small { color: var(--text-dim); font-size: 12px; }
  .layout-glyph { grid-column: 1; grid-row: 1 / span 2; width: 42px; height: 42px; display: grid; place-items: center; border-radius: 13px; background: var(--surface); color: var(--primary); font-size: 23px; }
  .theme { display: grid; gap: 8px; padding: 12px; text-align: left; border-radius: 12px; border: 2px solid; transition: transform .25s var(--spring), border-radius .3s var(--spring); }
  .theme:hover { transform: translateY(-2px); }
  .theme.on { border-radius: 18px; }
  .sw { display: flex; }
  .sw i { width: 22px; height: 22px; border-radius: 50%; margin-right: -6px; border: 2px solid rgba(255, 255, 255, .6); }
  .row.between { justify-content: space-between; }
  .badges { display: flex; gap: 10px; flex-wrap: wrap; margin-top: 10px; }
  .badge { width: 74px; height: 74px; display: grid; place-content: center; text-align: center; background: var(--surface-high); color: var(--text-dim); transition: transform .25s var(--spring); }
  .badge:hover { transform: scale(1.08) rotate(4deg); }
  .badge.have { background: var(--primary-container); color: var(--on-primary-container); }
  .badge.on { background: var(--primary); color: var(--on-primary); }
  .badge b { font-size: 24px; line-height: 1; }
  .badge small { font-size: 11px; margin-top: 3px; opacity: .85; }
</style>
