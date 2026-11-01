<script>
  // Expressive wavy progress bar: the wave scrolls while `animate` is true.
  let { fraction = 0, animate = true } = $props();
  const W = 1600;
  // the wave path never changes, so it is built once
  const d = (() => {
    let path = 'M0 10';
    for (let x = 0, up = true; x < W; x += 14, up = !up) path += ` q7 ${up ? -8 : 8} 14 0`;
    return path;
  })();
  const pct = $derived(Math.max(0.02, Math.min(1, fraction)) * 100);
</script>

<div class="wp">
  <div class="track"></div>
  <div class="active" style:width="{pct}%">
    <svg width={W} height="20" viewBox="0 0 {W} 20" class:anim={animate} aria-hidden="true">
      <path {d} fill="none" stroke="var(--primary)" stroke-width="5" stroke-linecap="round" />
    </svg>
  </div>
</div>

<style>
  .wp { position: relative; height: 20px; }
  .track { position: absolute; left: 0; right: 0; top: 7px; height: 6px; border-radius: 3px; background: var(--surface-high); }
  .active { position: absolute; left: 0; top: 0; height: 20px; overflow: hidden; transition: width .25s ease-out; }
  svg { display: block; }
  svg.anim { animation: slide .9s linear infinite; }
  @keyframes slide { to { transform: translateX(-28px); } }
</style>
