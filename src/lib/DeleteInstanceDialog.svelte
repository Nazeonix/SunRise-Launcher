<script>
  import Modal from './Modal.svelte';
  import Icon from './Icon.svelte';
  import { app, deleteInstance, toast } from './state.svelte.js';

  let { id } = $props();
  let deleteFiles = $state(false);
  let removing = $state(false);
  const instance = $derived(app.instances.find((item) => item.id === id));

  async function confirmDelete() {
    if (!instance || removing) return;
    removing = true;
    try {
      await deleteInstance(id, deleteFiles);
      app.dialog = null;
      app.deleteInstanceId = null;
    } catch (e) {
      toast(e);
    } finally {
      removing = false;
    }
  }
</script>

<Modal title="Delete instance" onclose={() => { app.dialog = null; app.deleteInstanceId = null; }}>
  {#if instance}
    <div class="warning">
      <span class="warning-icon"><Icon name="trash" size={22} /></span>
      <div><b>Remove “{instance.name}”?</b><p>This removes the instance from your launcher.</p></div>
    </div>
    <label class="delete-files">
      <input type="checkbox" bind:checked={deleteFiles} />
      <span><b>Also delete its folder</b><small>Includes worlds, mods, screenshots, and settings.</small></span>
    </label>
    <div class="foot">
      <button class="btn" onclick={() => { app.dialog = null; app.deleteInstanceId = null; }}>Cancel</button>
      <button class="btn danger" disabled={removing} onclick={confirmDelete}>{removing ? 'Deleting…' : 'Delete instance'}</button>
    </div>
  {/if}
</Modal>

<style>
  .warning { display: flex; align-items: center; gap: 14px; padding: 14px; border-radius: 18px 14px 18px 14px; background: color-mix(in srgb, var(--error) 10%, var(--surface)); }
  .warning-icon { width: 44px; height: 44px; display: grid; place-items: center; flex: none; border-radius: 15px 50% 50% 50%; color: var(--error); background: color-mix(in srgb, var(--error) 16%, var(--surface)); }
  .warning p { margin-top: 3px; color: var(--text-dim); font-size: 13px; }
  .delete-files { display: flex; gap: 11px; align-items: flex-start; margin-top: 16px; padding: 12px; border: 1px solid var(--outline); border-radius: 14px; cursor: pointer; }
  .delete-files input { margin: 3px 0 0; accent-color: var(--error); }
  .delete-files span { display: grid; gap: 3px; }
  .delete-files small { color: var(--text-dim); font-size: 12px; }
</style>
