<script>
  import Icon from './Icon.svelte';
  let { title, onclose, wide = false, children } = $props();
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') onclose?.(); }} />

<div class="backdrop" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) onclose?.(); }}>
  <div class="modal" class:wide role="dialog" aria-modal="true" aria-label={title}>
    <header>
      <h2>{title}</h2>
      <button class="icon-btn" onclick={() => onclose?.()} aria-label="Close"><Icon name="close" /></button>
    </header>
    <div class="body">{@render children?.()}</div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 50; display: grid; place-items: center;
    background: color-mix(in srgb, var(--scene-top) 45%, transparent);
    backdrop-filter: blur(3px);
    animation: fade .15s ease-out;
  }
  .modal {
    width: min(520px, 92vw); max-height: 88vh; display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--outline); border-radius: 20px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, .28);
    animation: pop .35s var(--spring);
  }
  .modal.wide { width: min(680px, 94vw); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 18px 20px 6px; }
  h2 { margin: 0; font-size: 22px; }
  .body { padding: 8px 20px 20px; overflow: auto; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes pop { from { transform: scale(.92); opacity: 0; } }
</style>
