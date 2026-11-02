<script>
  import { convertFileSrc } from '@tauri-apps/api/core';
  import Icon from './Icon.svelte';
  import { app, saveSettings, toast } from './state.svelte.js';
  import { api } from './api.js';

  let audio = $state();
  let open = $state(false);
  let playing = $state(false);
  let trackIndex = $state(0);
  let time = $state(0);
  let duration = $state(0);
  let volume = $state(0.7);

  const tracks = $derived(app.settings?.musicTracks ?? []);
  const current = $derived(tracks.length ? tracks[Math.min(trackIndex, tracks.length - 1)] : '');
  const src = $derived(current ? convertFileSrc(current) : '');
  const title = (path) => path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, '') || 'Unknown track';
  const clock = (n) => `${Math.floor((n || 0) / 60)}:${String(Math.floor((n || 0) % 60)).padStart(2, '0')}`;

  async function addTracks() {
    try {
      const chosen = await api.pickMusicFiles();
      if (!chosen.length) return;
      const merged = [...tracks, ...chosen.filter((path) => !tracks.includes(path))];
      await saveSettings({ musicTracks: merged });
      open = true;
      if (!current) trackIndex = 0;
      toast(`Added ${chosen.length} track${chosen.length === 1 ? '' : 's'}`);
    } catch (e) { toast(e); }
  }
  async function toggle() {
    if (!tracks.length) return addTracks();
    try {
      if (playing) { audio.pause(); playing = false; }
      else { await audio.play(); playing = true; }
    } catch (e) { toast(`Couldn't play this track: ${e}`); playing = false; }
  }
  async function selectTrack(index, autoplay = true) {
    if (!tracks.length) return;
    trackIndex = (index + tracks.length) % tracks.length;
    time = 0;
    if (autoplay) {
      await new Promise((resolve) => setTimeout(resolve, 0));
      try { await audio.play(); playing = true; } catch { playing = false; }
    }
  }
  async function removeTrack(index) {
    const next = tracks.filter((_, i) => i !== index);
    if (index === trackIndex) { audio.pause(); playing = false; }
    await saveSettings({ musicTracks: next });
    trackIndex = Math.max(0, Math.min(trackIndex - (index < trackIndex ? 1 : 0), next.length - 1));
  }
  function updateTime() { time = audio?.currentTime ?? 0; duration = audio?.duration || 0; }
  $effect(() => { if (audio) audio.volume = volume; });
</script>

<audio bind:this={audio} src={src} ontimeupdate={updateTime} onloadedmetadata={updateTime} onended={() => selectTrack(trackIndex + 1)} onerror={() => { playing = false; }}></audio>

<div class="player" class:expanded={open}>
  <div class="mini">
    <button class="album" aria-label="Toggle music player" onclick={() => open = !open}><Icon name="music" size={20} /></button>
    <button class="track" title={current ? title(current) : 'Add music'} onclick={() => open = !open}>
      <b>{current ? title(current) : 'Add music'}</b><small>{current ? `${trackIndex + 1} of ${tracks.length}` : 'Your local tracks'}</small>
    </button>
    <button class="transport" aria-label={playing ? 'Pause' : 'Play'} onclick={toggle}><Icon name={playing ? 'pause' : 'play'} size={19} /></button>
    <button class="icon-btn queue-toggle" aria-label={open ? 'Close music queue' : 'Open music queue'} aria-expanded={open} onclick={() => open = !open}><Icon name="chevron" size={18} /></button>
  </div>
  {#if open}
    <div class="details">
      <div class="controls">
        <button class="icon-btn" aria-label="Previous track" disabled={!tracks.length} onclick={() => selectTrack(trackIndex - 1)}><Icon name="back" size={18} /></button>
        <button class="transport main" aria-label={playing ? 'Pause' : 'Play'} onclick={toggle}><Icon name={playing ? 'pause' : 'play'} size={20} /></button>
        <button class="icon-btn" aria-label="Next track" disabled={!tracks.length} onclick={() => selectTrack(trackIndex + 1)}><Icon name="next" size={18} /></button>
      </div>
      <div class="timeline"><span>{clock(time)}</span><input type="range" min="0" max={duration || 1} step="1" value={time} oninput={(e) => { if (audio) audio.currentTime = Number(e.currentTarget.value); }} aria-label="Track position" /><span>{clock(duration)}</span></div>
      <div class="queue-head"><b>Queue · {tracks.length}</b><button class="btn sm" onclick={addTracks}><Icon name="plus" size={16} /> Add music</button></div>
      <div class="queue">
        {#each tracks as path, i (path)}
          <div class="queue-row" class:current={i === trackIndex}>
            <button class="queue-track" onclick={() => selectTrack(i)}><span class="queue-num">{i === trackIndex && playing ? '♫' : String(i + 1).padStart(2, '0')}</span><span>{title(path)}</span></button>
            <button class="icon-btn remove" aria-label="Remove {title(path)}" onclick={() => removeTrack(i)}>×</button>
          </div>
        {:else}
          <p class="hint queue-empty">Add MP3, OGG, WAV, FLAC, M4A, or AAC files from your computer.</p>
        {/each}
      </div>
      <label class="volume"><Icon name="volume" size={17} /><input type="range" min="0" max="1" step="0.01" bind:value={volume} aria-label="Volume" /></label>
    </div>
  {/if}
</div>

<style>
  audio { display: none; }
  .player { position: fixed; z-index: 30; right: 24px; bottom: 22px; width: 300px; background: var(--surface); border: 1px solid var(--outline); border-radius: 20px 16px 22px 16px; box-shadow: 0 12px 34px rgba(0,0,0,.2); overflow: hidden; }
  .mini { display: flex; align-items: center; gap: 8px; padding: 8px; }
  .album { width: 42px; height: 42px; flex: none; display: grid; place-items: center; color: var(--on-primary); background: var(--primary); border-radius: 15px 12px 16px 12px; }
  .track { display: grid; text-align: left; flex: 1; min-width: 0; }
  .track b { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 13px; }
  .track small { color: var(--text-dim); font-size: 11px; }
  .transport { width: 36px; height: 36px; flex: none; display: grid; place-items: center; color: var(--on-primary); background: var(--primary); border-radius: 50%; }
  .queue-toggle { transition: transform .25s var(--spring); }
  .expanded .queue-toggle { transform: rotate(180deg); }
  .details { border-top: 1px solid var(--outline); padding: 12px; }
  .controls { display: flex; align-items: center; justify-content: center; gap: 14px; }
  .controls .main { width: 42px; height: 42px; }
  .controls button:disabled { opacity: .4; }
  .timeline { display: flex; align-items: center; gap: 8px; margin: 8px 0 12px; color: var(--text-dim); font: 11px ui-monospace, monospace; }
  .timeline input { min-width: 0; }
  .queue-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 6px; font-size: 12px; }
  .queue { max-height: 170px; overflow-y: auto; }
  .queue-row { display: flex; align-items: center; border-radius: 10px; }
  .queue-row.current { background: var(--primary-container); color: var(--on-primary-container); }
  .queue-track { display: flex; align-items: center; gap: 9px; flex: 1; min-width: 0; padding: 8px 6px; text-align: left; font-size: 12px; }
  .queue-track > span:last-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .queue-num { width: 22px; flex: none; text-align: center; color: var(--primary); font-weight: 750; }
  .remove { width: 30px; height: 30px; font-size: 20px; }
  .queue-empty { padding: 10px 4px; font-size: 12px; }
  .volume { display: flex; align-items: center; gap: 8px; margin-top: 7px; color: var(--text-dim); }
  @media (max-width: 700px) { .player { right: 12px; bottom: 12px; width: min(300px, calc(100vw - 24px)); } }
</style>
