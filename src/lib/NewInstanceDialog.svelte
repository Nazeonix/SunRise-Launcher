<script>
  import Modal from './Modal.svelte';
  import InstanceForm from './InstanceForm.svelte';
  import { app, createInstance, loadVersions, toast } from './state.svelte.js';

  const latest = () => app.versions.find((v) => v.type === 'release')?.id ?? '';
  let form = $state({ name: '', mcVersion: latest(), loader: 'vanilla', loaderVersion: '', javaPath: '', ramMb: 0, color: '#6A52B3', dirParent: '' });
  let busy = $state(false);

  loadVersions();
  // the version list may still be loading when the dialog opens
  $effect(() => {
    if (!form.mcVersion && app.versions.length) form.mcVersion = latest();
  });

  async function create() {
    if (!form.mcVersion) return toast('Pick a Minecraft version first');
    busy = true;
    try {
      await createInstance({ ...$state.snapshot(form), name: form.name.trim() || `Minecraft ${form.mcVersion}` });
      app.dialog = null;
    } catch (e) {
      toast(e);
    }
    busy = false;
  }
</script>

<Modal title="New instance" onclose={() => (app.dialog = null)}>
  <InstanceForm bind:form showFolder />
  <div class="foot">
    <button class="btn" onclick={() => (app.dialog = null)}>Cancel</button>
    <button class="btn primary" disabled={busy || !form.mcVersion} onclick={create}>Create</button>
  </div>
</Modal>
