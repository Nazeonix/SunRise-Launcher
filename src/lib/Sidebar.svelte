<script>
  import { app, selectInstance, selectAccount, updateInstance, toast } from './state.svelte.js';
  import { api } from './api.js';
  import Icon from './Icon.svelte';
  import Avatar from './Avatar.svelte';
  import { cookie } from './shapes.js';
  import { convertFileSrc } from '@tauri-apps/api/core';

  const LOADERS = { vanilla: 'Vanilla', fabric: 'Fabric', forge: 'Forge', neoforge: 'NeoForge' };
  let q = $state('');
  const list = $derived(
    app.instances.filter((i) => `${i.name} ${i.mcVersion} ${i.loader}`.toLowerCase().includes(q.trim().toLowerCase()))
  );
  const accountName = $derived(app.account?.username ?? app.settings?.username ?? 'Player');
  const providerName = (key) => ({ microsoft: 'Microsoft', elyby: 'Ely.by', littleskin: 'LittleSkin' }[key] ?? key);
  const INSTANCE_COLORS = ['#6A52B3', '#2E6B34', '#2B5EA7', '#C2562B', '#B3487A', '#0E8A8A', '#B8860B', '#5A5A66'];
  const imageSrc = (path) => path ? (/^(https?:|data:|asset:)/i.test(path) ? path : convertFileSrc(path)) : '';
  let instanceMenu = $state(null);
  let styleOpen = $state(false);
  function toggleAccountSwitcher(event) {
    event.stopPropagation();
    app.accountSwitcherOpen = !app.accountSwitcherOpen;
  }
  function addAccount() {
    app.accountSwitcherOpen = false;
    app.dialog = 'account-add';
  }
  function manageAccounts() {
    app.accountSwitcherOpen = false;
    app.dialog = 'account';
  }
  function handleWindowClick(event) {
    if (!event.target.closest?.('.account-wrap')) app.accountSwitcherOpen = false;
    if (!event.target.closest?.('.instance-context')) {
      instanceMenu = null;
      styleOpen = false;
    }
  }
  function openInstanceMenu(event, instance) {
    event.preventDefault();
    selectInstance(instance.id);
    instanceMenu = {
      id: instance.id,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - 252)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - 300)),
    };
    styleOpen = false;
  }
  async function setInstanceColor(instance, color) {
    instanceMenu = null;
    styleOpen = false;
    try { await updateInstance({ ...instance, color }); } catch (e) { toast(e); }
  }
  function openInstanceFolder(id) {
    instanceMenu = null;
    api.openFolder(id, 'game').catch(toast);
  }
  function requestDelete(id) {
    instanceMenu = null;
    app.deleteInstanceId = id;
    app.dialog = 'delete-instance';
  }
</script>

<svelte:window onclick={handleWindowClick} />

<aside>
  <div class="brand">
    <img class="logo" src="/icon.svg" alt="" width="44" height="44" />
    <span>Sunrise</span>
  </div>

  <div class="head">
    <span class="label">Instances</span>
    <span class="count">{app.instances.length}</span>
    <button class="icon-btn" title="New instance" aria-label="New instance" onclick={() => (app.dialog = 'new')}><Icon name="plus" /></button>
  </div>

  <label class="search">
    <Icon name="search" size={16} />
    <input placeholder="Search instances" bind:value={q} />
  </label>

  <div class="list" class:grid={app.settings?.instanceLayout === 'grid'}>
    {#each list as inst (inst.id)}
      {@const g = app.game[inst.id]}
      <button class="item" class:active={inst.id === app.selectedId} onclick={() => selectInstance(inst.id)} oncontextmenu={(event) => openInstanceMenu(event, inst)}>
        {#if inst.iconUrl}<img class="ico custom" src={imageSrc(inst.iconUrl)} alt="" />{:else}<span class="ico" style:background={inst.color} style:clip-path={cookie(8, 0.07)}>{inst.name.slice(0, 1).toUpperCase()}</span>{/if}
        <span class="meta">
          <span class="name">{inst.name}</span>
          <span class="sub">{inst.mcVersion} · {LOADERS[inst.loader] ?? inst.loader}</span>
        </span>
        {#if g?.state === 'running'}
          <span class="dot" title="Running"></span>
        {:else if g?.state === 'installing'}
          <span class="spin"></span>
        {/if}
      </button>
    {:else}
      <p class="hint empty">{q ? 'No matches.' : 'No instances yet.'}</p>
    {/each}
  </div>

  {#if instanceMenu}
    {@const contextInstance = app.instances.find((inst) => inst.id === instanceMenu.id)}
    {#if contextInstance}
      {@const contextGame = app.game[contextInstance.id]}
      <div class="instance-context" style={`left:${instanceMenu.x}px;top:${instanceMenu.y}px`}>
        <div class="context-heading"><span class="context-icon" style:background={contextInstance.color} style:clip-path={cookie(8, 0.07)}>{contextInstance.name.slice(0, 1).toUpperCase()}</span><span><b>{contextInstance.name}</b><small>{contextInstance.mcVersion}</small></span></div>
        <div class="context-divider"></div>
        <button class="context-item" aria-expanded={styleOpen} onclick={() => (styleOpen = !styleOpen)}><Icon name="sparkle" size={18} /><span>Change style</span><span class="context-arrow"><Icon name="chevron" size={16} /></span></button>
        {#if styleOpen}
          <div class="context-colors" aria-label="Choose instance color">
            {#each INSTANCE_COLORS as color (color)}
              <button class="context-swatch" class:selected={contextInstance.color === color} style:background={color} aria-label="Set color {color}" title={color} onclick={() => setInstanceColor(contextInstance, color)}></button>
            {/each}
          </div>
        {/if}
        <button class="context-item" onclick={() => openInstanceFolder(contextInstance.id)}><Icon name="folder" size={18} /><span>Open root folder</span></button>
        <button class="context-item destructive" disabled={contextGame?.state === 'running' || contextGame?.state === 'installing'} title={contextGame?.state === 'running' || contextGame?.state === 'installing' ? 'Stop Minecraft before deleting' : ''} onclick={() => requestDelete(contextInstance.id)}><Icon name="trash" size={18} /><span>Delete instance</span></button>
      </div>
    {/if}
  {/if}

  <div class="newrow">
    <button class="btn primary" onclick={() => (app.dialog = 'new')}><Icon name="plus" /> New</button>
    <button class="btn" onclick={() => (app.dialog = 'modpacks')} title="Browse modpacks from Modrinth and CurseForge"><Icon name="puzzle" /> Modpacks</button>
  </div>

  <div class="foot">
    <div class="account-wrap">
      <button class="account" aria-expanded={app.accountSwitcherOpen} aria-haspopup="dialog" onclick={toggleAccountSwitcher}>
        <Avatar size={36} skinUrl={app.account?.skinUrl} />
        <span class="who">
          <b>{accountName}</b>
          <small><i class="status" class:ok={!!app.account}></i>{app.account ? providerName(app.account.provider) : 'Offline'}</small>
        </span>
      </button>
      {#if app.accountSwitcherOpen}
        <div class="switcher" role="dialog" aria-label="Account switcher">
          {#if app.accounts.length > 1}
            <b class="switch-title">Switch account</b>
            {#each app.accounts as account (account.id)}
              <button class="choice" class:chosen={account.id === app.activeAccountId} onclick={() => selectAccount(account.id)}>
                <Avatar size={34} skinUrl={account.skinUrl} />
                <span class="choice-meta"><b>{account.username}</b><small>{providerName(account.provider)}</small></span>
                {#if account.id === app.activeAccountId}<span class="check">✓</span>{/if}
              </button>
            {/each}
          {:else if app.accounts.length === 1}
            <b class="switch-title">You're signed in as</b>
            <div class="single-account"><Avatar size={36} skinUrl={app.accounts[0].skinUrl} /><span><b>{app.accounts[0].username}</b><small>{providerName(app.accounts[0].provider)}</small></span></div>
            <p class="hint switch-hint">Add another account to switch between accounts.</p>
          {:else}
            <b class="switch-title">No signed-in accounts</b>
            <p class="hint switch-hint">Add an account to play online.</p>
          {/if}
          <button class="btn primary add-account" onclick={addAccount}>Add another account</button>
          {#if app.accounts.length}<button class="manage" onclick={manageAccounts}>Manage accounts</button>{/if}
        </div>
      {/if}
    </div>
    <button class="icon-btn" title="Settings" aria-label="Settings" onclick={() => (app.dialog = 'settings')}><Icon name="gear" /></button>
  </div>
</aside>

<style>
  aside {
    display: flex; flex-direction: column; gap: 12px; padding: 16px 14px 14px;
    background: var(--surface); border-right: 1px solid var(--outline); height: 100vh; min-height: 0;
  }
  .brand { display: flex; align-items: center; gap: 12px; padding: 2px 6px 6px; font-size: 22px; font-weight: 800; letter-spacing: .2px; }
  .logo { width: 44px; height: 44px; border-radius: 10px; image-rendering: pixelated; flex: none; }
  .head { display: flex; align-items: center; gap: 8px; padding: 0 2px 0 6px; }
  .head .label { flex: 1; text-transform: uppercase; letter-spacing: .6px; font-size: 12px; }
  .count { font-size: 12px; font-weight: 700; color: var(--on-primary-container); background: var(--primary-container); border-radius: 8px; padding: 2px 8px; }
  .search { display: flex; align-items: center; gap: 8px; height: 38px; padding: 0 12px; border-radius: 10px; background: var(--bg); border: 1px solid var(--outline); color: var(--text-dim); }
  .search input { flex: 1; min-width: 0; border: 0; background: none; outline: none; }
  .list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 4px; margin: 0 -4px; padding: 0 4px; }
  .list.grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); align-content: start; }
  .list.grid .item { min-width: 0; flex-direction: column; align-items: flex-start; gap: 7px; padding: 10px; border: 1px solid var(--outline); background: var(--surface-high); }
  .list.grid .item.active { background: var(--primary-container); border-color: transparent; }
  .list.grid .ico { width: 100%; height: auto; aspect-ratio: 1; font-size: 22px; border-radius: 13px; }
  .list.grid .ico.custom { object-fit: cover; }
  .list.grid .meta { width: 100%; }
  .item {
    display: flex; align-items: center; gap: 12px; padding: 8px 10px; border-radius: 12px; text-align: left; width: 100%;
    transition: background .15s, border-radius .3s var(--spring);
  }
  .item:hover { background: var(--secondary-container); }
  .item.active { background: var(--primary-container); color: var(--on-primary-container); border-radius: 16px; }
  .ico { width: 40px; height: 40px; flex: none; display: grid; place-items: center; color: #fff; font-weight: 800; font-size: 17px; }
  .meta { display: grid; min-width: 0; flex: 1; }
  .name { font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { font-size: 12.5px; opacity: .72; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: #4CAF50; box-shadow: 0 0 0 3px color-mix(in srgb, #4CAF50 25%, transparent); flex: none; }
  .empty { padding: 8px 10px; }
  .newrow { display: grid; grid-template-columns: 1fr 1.4fr; gap: 8px; }
  .newrow .btn { height: 42px; padding: 0 10px; }
  .foot { display: flex; align-items: center; gap: 6px; padding-top: 12px; border-top: 1px solid var(--outline); }
  .account-wrap { position: relative; flex: 1; min-width: 0; }
  .account { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; padding: 6px; border-radius: 12px; text-align: left; transition: background .15s; }
  .account:hover { background: var(--secondary-container); }
  .switcher { position: absolute; left: 0; bottom: calc(100% + 10px); z-index: 20; width: 300px; max-width: calc(100vw - 32px); padding: 12px; display: grid; gap: 8px; background: var(--surface); color: var(--text); border: 1px solid var(--outline); border-radius: 16px; box-shadow: 0 12px 32px rgba(0,0,0,.22); }
  .switch-title { font-size: 13px; padding: 2px 4px; }
  .choice { display: flex; align-items: center; gap: 10px; text-align: left; padding: 6px; border-radius: 10px; }
  .choice:hover, .choice.chosen { background: var(--primary-container); color: var(--on-primary-container); }
  .choice-meta { display: grid; min-width: 0; flex: 1; }
  .choice-meta b, .single-account b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .choice-meta small, .single-account small { color: var(--text-dim); font-size: 12px; }
  .check { color: var(--primary); font-weight: 800; }
  .single-account { display: flex; align-items: center; gap: 10px; padding: 4px; }
  .single-account > span { display: grid; min-width: 0; }
  .switch-hint { padding: 0 4px 2px; }
  .add-account { width: 100%; }
  .manage { color: var(--text-dim); font-size: 13px; padding: 4px; }
  .manage:hover { color: var(--primary); }
  .instance-context { position: fixed; z-index: 40; width: 244px; padding: 8px; display: grid; gap: 3px; border: 1px solid var(--outline); border-radius: 17px 13px 19px 13px; background: var(--surface); color: var(--text); box-shadow: 0 14px 36px rgba(0,0,0,.24); animation: context-pop .18s var(--spring); }
  .context-heading { display: flex; align-items: center; gap: 10px; padding: 6px 7px 8px; min-width: 0; }
  .context-icon { width: 34px; height: 34px; flex: none; display: grid; place-items: center; color: white; font-weight: 800; border-radius: 10px; }
  .context-heading > span:last-child { display: grid; min-width: 0; }
  .context-heading b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
  .context-heading small { color: var(--text-dim); font-size: 11px; }
  .context-divider { height: 1px; margin: 0 4px 3px; background: var(--outline); }
  .context-item { min-height: 38px; width: 100%; display: flex; align-items: center; gap: 10px; padding: 0 9px; border-radius: 10px; text-align: left; color: var(--text); font-size: 13px; }
  .context-item span:nth-child(2) { flex: 1; }
  .context-item:hover:not(:disabled) { background: var(--secondary-container); color: var(--on-secondary-container); }
  .context-item:disabled { opacity: .48; cursor: not-allowed; }
  .context-item.destructive { color: var(--error); }
  .context-item.destructive:hover:not(:disabled) { background: color-mix(in srgb, var(--error) 12%, var(--surface)); }
  .context-arrow { display: grid; place-items: center; transform: rotate(-90deg); color: var(--text-dim); }
  .context-colors { display: flex; flex-wrap: wrap; gap: 8px; padding: 8px 9px 10px; margin: 0 4px 3px; border-radius: 11px; background: var(--surface-high); }
  .context-swatch { width: 24px; height: 24px; border-radius: 8px; border: 2px solid transparent; transition: transform .18s var(--spring), border-radius .2s var(--spring); }
  .context-swatch:hover { transform: scale(1.14); border-radius: 50%; }
  .context-swatch.selected { border-color: var(--text); box-shadow: 0 0 0 2px var(--surface-high); }
  @keyframes context-pop { from { transform: translateY(-4px) scale(.96); opacity: 0; } }
  .who { display: grid; min-width: 0; }
  .who b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .who small { color: var(--text-dim); display: flex; align-items: center; gap: 6px; }
  .status { width: 8px; height: 8px; border-radius: 50%; background: #F2A63B; display: inline-block; }
  .status.ok { background: #4CAF50; }
</style>
