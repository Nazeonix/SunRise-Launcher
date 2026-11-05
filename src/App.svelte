<script>
  import { onMount } from 'svelte';
  import { app, init } from './lib/state.svelte.js';
  import { applyTheme } from './lib/themes.js';
  import Sidebar from './lib/Sidebar.svelte';
  import InstanceView from './lib/InstanceView.svelte';
  import NewInstanceDialog from './lib/NewInstanceDialog.svelte';
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import AccountDialog from './lib/AccountDialog.svelte';
  import DeleteInstanceDialog from './lib/DeleteInstanceDialog.svelte';
  import ModpackBrowser from './lib/ModpackBrowser.svelte';
  import MusicPlayer from './lib/MusicPlayer.svelte';
  import HomeLaunch from './lib/HomeLaunch.svelte';

  const selected = $derived(app.instances.find((i) => i.id === app.selectedId));

  onMount(() => { init(); });
  $effect(() => { if (app.settings) applyTheme(app.settings.theme, app.settings.mode, app.systemDark); });
</script>

<div class="shell">
  <Sidebar />
  <main>
    {#if !app.ready}
      <div class="center"><span class="spin big"></span></div>
    {:else if app.error}
      <div class="center"><p class="err">{app.error}</p></div>
    {:else if selected}
      {#key selected.id}
        <InstanceView instance={selected} />
      {/key}
    {:else}
      <HomeLaunch />
    {/if}
  </main>

  {#if app.dialog === 'new'}<NewInstanceDialog />{/if}
  {#if app.dialog === 'settings'}<SettingsDialog />{/if}
  {#if app.dialog === 'account' || app.dialog === 'account-add'}<AccountDialog startAdding={app.dialog === 'account-add'} />{/if}
  {#if app.dialog === 'delete-instance'}<DeleteInstanceDialog id={app.deleteInstanceId} />{/if}
  {#if app.dialog === 'modpacks'}<ModpackBrowser />{/if}
  {#if app.ready}<MusicPlayer />{/if}
  {#if app.toast}<div class="toast">{app.toast}</div>{/if}
</div>

<style>
  .shell { display: grid; grid-template-columns: 284px 1fr; height: 100vh; }
  main { overflow-y: auto; padding: 22px 28px 28px; }
  .center { height: 100%; display: grid; place-content: center; justify-items: center; gap: 12px; text-align: center; max-width: 420px; margin: 0 auto; }
  .big { width: 34px; height: 34px; color: var(--primary); }
</style>
