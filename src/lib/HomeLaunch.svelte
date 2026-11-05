<script>
  import { app, selectInstance, play, stop, toast } from './state.svelte.js';
  import { api } from './api.js';
  import Icon from './Icon.svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';

  let id = $state(app.instances[0]?.id ?? null);
  $effect(() => { if (!app.instances.some((i) => i.id === id)) id = app.instances[0]?.id ?? null; });
  const instance = $derived(app.instances.find((i) => i.id === id));
  const game = $derived(instance ? app.game[instance.id] ?? { state: 'idle' } : { state: 'idle' });
  const profiles = $derived(instance?.launchProfiles ?? []);
  let profileName = $state('');
  const profile = $derived(profiles.find((p) => p.name === profileName) ?? null);
  const source = (path) => path ? (/^(https?:|data:|asset:)/i.test(path) ? path : convertFileSrc(path)) : '';
  const loaderName = (i) => ({ vanilla: 'Vanilla', fabric: 'Fabric', forge: 'Forge', neoforge: 'NeoForge' }[i?.loader] ?? i?.loader);
  function openInstance(tab) { if (!instance) return; selectInstance(instance.id); app.instanceTab = tab; }
</script>

<div class="home">
  <div class="welcome"><img src="/icon.svg" alt="" width="38" height="38" /><div><span class="eyebrow">SUNRISE LAUNCHER</span><h1>Ready when you are.</h1></div></div>
  {#if instance}
    <section class="launch-card" style:--cover={instance.bannerUrl ? `url('${source(instance.bannerUrl)}')` : 'none'}>
      <div class="cover">
        <div class="instance-id">
          {#if instance.iconUrl}<img src={source(instance.iconUrl)} alt="" />{:else}<span style:background={instance.color}>{instance.name.slice(0,1).toUpperCase()}</span>{/if}
          <div><small>SELECTED INSTANCE</small><b>{instance.name}</b><span>{instance.mcVersion} · {loaderName(instance)}</span></div>
        </div>
      </div>
      <div class="controls">
        <label><span>Instance</span><select bind:value={id}>{#each app.instances as item (item.id)}<option value={item.id}>{item.name} · {item.mcVersion}</option>{/each}</select></label>
        <label><span>Launch profile</span><select bind:value={profileName}><option value="">Default settings</option>{#each profiles as p (p.name)}<option value={p.name}>{p.name}</option>{/each}</select></label>
        <button class="launch" class:stop={game.state === 'running'} disabled={game.state === 'installing'} onclick={() => game.state === 'running' ? stop(instance.id) : play(instance.id, profile)}>
          <Icon name={game.state === 'running' ? 'stop' : 'play'} size={23} />{game.state === 'installing' ? (app.progress[instance.id]?.text || 'Preparing…') : game.state === 'running' ? 'Stop Minecraft' : 'Play'}
        </button>
        <div class="quick"><button onclick={() => openInstance('mods')}><Icon name="puzzle" />Mods</button><button onclick={() => openInstance('worlds')}><Icon name="world" />Worlds</button><button onclick={() => api.openFolder(instance.id, 'game').catch(toast)}><Icon name="folder" />Folder</button><button onclick={() => openInstance('settings')}><Icon name="gear" />Settings</button></div>
        <div class="account"><span>PLAYING AS</span><b>{app.account?.username ?? app.settings?.username ?? 'Player'}</b><button onclick={() => (app.dialog = 'account')}><Icon name="user" size={18} />Switch account</button></div>
      </div>
    </section>
  {:else}
    <section class="empty"><div class="empty-icon"><Icon name="sun" size={34} /></div><h2>Start with an instance</h2><p>Create a Minecraft setup with its own version, mods and worlds.</p><button class="launch" onclick={() => (app.dialog = 'new')}><Icon name="plus" />Create instance</button></section>
  {/if}
</div>

<style>
  .home{max-width:850px;margin:0 auto;min-height:100%;display:grid;align-content:center;gap:28px;padding:28px 0}.welcome{display:flex;align-items:center;gap:14px;color:var(--primary)}.eyebrow{font-size:11px;letter-spacing:.14em;font-weight:800;color:var(--text-dim)}h1{font-size:30px;letter-spacing:-.035em;margin:3px 0 0;color:var(--text)}.launch-card,.empty{overflow:hidden;border-radius:30px;background:var(--surface-container);border:1px solid var(--outline-variant);box-shadow:0 22px 60px #14102118}.cover{min-height:190px;display:flex;align-items:end;padding:24px;background:linear-gradient(90deg,#241737dd,#24173735),linear-gradient(0deg,#15101ee6,transparent 75%),var(--cover),linear-gradient(130deg,var(--primary-container),var(--surface-container-high));background-size:cover;background-position:center;position:relative}.cover:after{content:"";position:absolute;inset:0;background:radial-gradient(ellipse at 85% 15%,#ffffff23,transparent 38%);pointer-events:none}.instance-id{display:flex;align-items:center;gap:14px;position:relative;z-index:1;color:white}.instance-id img,.instance-id>span{width:60px;height:60px;object-fit:cover;border-radius:20px;display:grid;place-items:center;font-weight:800;font-size:25px;box-shadow:0 6px 22px #0004}.instance-id div{display:grid;gap:3px}.instance-id small,.account>span{font-size:10px;letter-spacing:.14em;font-weight:800;opacity:.78}.instance-id b{font-size:25px}.instance-id div span{font-size:13px;opacity:.82}.controls{padding:23px;display:grid;grid-template-columns:1fr 1fr;gap:14px}.controls label{display:grid;gap:7px;color:var(--text-dim);font-size:12px;font-weight:700}.controls select{height:46px;padding:0 13px;border-radius:14px;background:var(--surface-high);color:var(--text);border:1px solid var(--outline-variant);font:inherit}.launch{grid-column:1/-1;display:flex;justify-content:center;align-items:center;gap:10px;min-height:58px;border-radius:18px;background:var(--primary);color:var(--on-primary);font-size:18px;font-weight:800;transition:transform .18s var(--spring),border-radius .25s var(--spring)}.launch:hover{transform:translateY(-2px);border-radius:24px}.launch:disabled{opacity:.7}.launch.stop{background:var(--tertiary-container);color:var(--on-tertiary-container)}.quick{display:grid;grid-template-columns:repeat(4,1fr);gap:9px}.quick button,.account button{display:flex;justify-content:center;align-items:center;gap:8px;min-height:44px;border-radius:14px;background:var(--surface-high);color:var(--text);font-weight:650}.quick button:hover,.account button:hover{background:var(--secondary-container)}.account{grid-column:1/-1;padding-top:13px;border-top:1px solid var(--outline-variant);display:flex;align-items:center;gap:10px}.account>span{color:var(--text-dim)}.account b{margin-right:auto}.account button{padding:0 12px;min-height:38px;font-size:12px}.empty{padding:48px;display:grid;justify-items:center;text-align:center;gap:12px}.empty-icon{width:72px;height:72px;display:grid;place-items:center;border-radius:26px;background:var(--primary-container);color:var(--on-primary-container)}.empty h2{margin:4px 0}.empty p{color:var(--text-dim);max-width:360px}.empty .launch{grid-column:auto;padding:0 24px;margin-top:8px}@media(max-width:650px){.controls{grid-template-columns:1fr;padding:16px}.quick{grid-template-columns:repeat(2,1fr)}.cover{min-height:155px}.account{flex-wrap:wrap}.account>span{width:100%}.home{padding:12px 0}}
</style>
