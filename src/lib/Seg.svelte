<script>
  // connected button group (Material 3 Expressive)
  let { options, value = $bindable(), onchange } = $props();
</script>

<div class="seg">
  {#each options as o (o.value)}
    <button type="button" class:on={value === o.value} onclick={() => { value = o.value; onchange?.(o.value); }}>{o.label}</button>
  {/each}
</div>
