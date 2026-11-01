<script>
  let { size = 36, skinUrl = '' } = $props();
  let failed = $state(false);
  $effect(() => { skinUrl; failed = false; });
  const FACE = ['hhhhhhhh', 'hhhhhhhh', 'hssssssh', 'ssssssss', 'swbssbws', 'ssssssss', 'sssmmsss', 'hssssssh'];
  const COL = { h: '#4A2E1B', s: '#D9A77C', w: '#FFFFFF', b: '#3A4FA0', m: '#8B4A3A' };
</script>

{#if skinUrl && !failed}
  <span class="head" style:width="{size}px" style:height="{size}px">
    <img class="skin face" src={skinUrl} alt="" onerror={() => (failed = true)} />
    <img class="skin hat" src={skinUrl} alt="" onerror={() => (failed = true)} />
  </span>
{:else}
  <svg width={size} height={size} viewBox="0 0 8 8" shape-rendering="crispEdges" style="border-radius:22%;background:#2B222F;flex:none">
    {#each FACE as row, y}
      {#each row.split('') as ch, x}
        <rect {x} {y} width="1.02" height="1.02" fill={COL[ch]} />
      {/each}
    {/each}
  </svg>
{/if}

<style>
  .head { position: relative; display: block; overflow: hidden; border-radius: 22%; background: #2B222F; flex: none; image-rendering: pixelated; }
  .skin { position: absolute; width: 800%; height: 800%; max-width: none; image-rendering: pixelated; }
  .face { left: -100%; top: -100%; }
  .hat { left: -500%; top: -100%; }
</style>
