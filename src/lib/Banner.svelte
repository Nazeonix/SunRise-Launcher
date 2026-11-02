<script>
  // Procedural pixel-art sunrise, coloured by the active theme.
  import { app } from './state.svelte.js';
  import { themeOf } from './themes.js';
  import { convertFileSrc } from '@tauri-apps/api/core';

  let { title = '', subtitle = '', height = 200, imageUrl = '' } = $props();
  let canvas = $state();
  let width = $state(0);
  let imageFailed = $state(false);
  const showImage = $derived(Boolean(imageUrl) && !imageFailed);
  const displayUrl = $derived(imageUrl ? (/^(https?:|data:|asset:)/i.test(imageUrl) ? imageUrl : convertFileSrc(imageUrl)) : '');
  const th = $derived(themeOf(app.settings?.theme, app.settings?.mode, app.systemDark));

  $effect(() => {
    imageUrl;
    imageFailed = false;
  });

  const hex = (h) => { const n = parseInt(h.slice(1), 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]; };
  const mix = (a, b, t) => { const A = hex(a), B = hex(b); return `rgb(${A.map((v, i) => Math.round(v + (B[i] - v) * t)).join(',')})`; };
  const rgba = (h, a) => { const [r, g, b] = hex(h); return `rgba(${r},${g},${b},${a})`; };

  function draw(w, h, t) {
    const cell = 6;
    const cols = Math.max(40, Math.round(w / cell));
    const rows = Math.max(20, Math.round(h / cell));
    canvas.width = cols;
    canvas.height = rows;
    const ctx = canvas.getContext('2d');
    const top = t.sceneTop, glow = t.sceneGlow;
    const far = mix(top, glow, 0.30), near = mix(top, '#000000', 0.35), water = mix(top, glow, 0.18);
    const hz = Math.round(rows * 0.74);

    // sky + water
    for (let r = 0; r < rows; r++) {
      ctx.fillStyle = r < hz ? mix(top, glow, Math.pow(r / hz, 2)) : mix(water, top, ((r - hz) / Math.max(1, rows - hz)) * 0.6);
      ctx.fillRect(0, r, cols, 1);
    }

    // sun + pixel glow
    const sunN = Math.max(2, Math.round(rows * 0.15));
    const sx = Math.round(cols * 0.8);
    const sy = hz - Math.round(sunN * 1.9);
    for (let k = 3; k >= 1; k--) {
      ctx.fillStyle = rgba(glow, 0.16);
      ctx.fillRect(sx - Math.floor(sunN / 2) - 2 * k, sy - 2 * k, sunN + 4 * k, sunN + 4 * k);
    }
    ctx.fillStyle = '#FFF1A8';
    ctx.fillRect(sx - Math.floor(sunN / 2), sy, sunN, sunN);

    // clouds
    ctx.globalAlpha = 0.85;
    ctx.fillStyle = mix(glow, '#ffffff', 0.55);
    for (const [fx, fy, len] of [[0.50, 0.10, 9], [0.72, 0.05, 12], [0.86, 0.17, 7], [0.30, 0.14, 7]]) {
      const x = Math.round(cols * fx), y = Math.round(rows * fy);
      ctx.fillRect(x, y, len, 1);
      ctx.fillRect(x + 2, y - 1, len - 4, 1);
    }
    ctx.globalAlpha = 1;

    // mountains (far, then near) with a valley around the sun
    for (let c = 0; c < cols; c++) {
      const u = c / cols;
      const valley = 1 - 0.75 * Math.exp(-Math.pow((u - 0.8) / 0.11, 2));
      const fh = (0.30 + 0.16 * (Math.sin(u * 9 + 0.6) + 0.6 * Math.sin(u * 21 + 2.1) + 0.3 * Math.sin(u * 43))) * valley;
      const fr = Math.max(1, Math.round(fh * hz));
      ctx.fillStyle = far;
      ctx.fillRect(c, hz - fr, 1, fr);
      const nh = (0.12 + 0.10 * (Math.sin(u * 13 + 1.3) + 0.5 * Math.sin(u * 29 + 0.2))) * Math.max(0.5, valley);
      const nr = Math.max(1, Math.round(nh * hz));
      ctx.fillStyle = near;
      ctx.fillRect(c, hz - nr, 1, nr);
    }

    // cliff + trees on the right
    const greens = ['#2F6B3A', '#3C8446', '#265A33'];
    for (let c = 0; c < cols; c++) {
      if (c / cols <= 0.86) continue;
      ctx.fillStyle = '#4B7F3A';
      ctx.fillRect(c, hz - 2, 1, 3);
      ctx.fillStyle = '#6B4A33';
      ctx.fillRect(c, hz + 1, 1, 4);
      if (c % 5 === 1) {
        const hsh = (Math.imul(c, 2654435761) >>> 16) % 7;
        const hgt = 3 + hsh;
        ctx.fillStyle = '#5B3A29';
        ctx.fillRect(c, hz - (hgt + 2), 1, hgt);
        ctx.fillStyle = greens[hsh % 3];
        ctx.fillRect(c - 1, hz - (hgt + 5), 3, 3);
      }
    }

    // sun reflection
    ctx.fillStyle = rgba('#FFF1A8', 0.55);
    for (let r = hz; r < rows; r++) {
      const f = (r - hz) / Math.max(1, rows - hz);
      let wpx = sunN * (1 - f * 0.6);
      if (r % 2 === 0) wpx *= 0.6;
      ctx.fillRect(Math.round(sx - wpx / 2), r, Math.max(1, Math.round(wpx)), 1);
    }
  }

  $effect(() => {
    if (!showImage && canvas && width > 0) draw(width, height, th);
  });
</script>

<div class="banner" class:image={showImage} style:height="{height}px" bind:clientWidth={width}>
  {#if showImage}
    <img class="screenshot" src={displayUrl} alt="" onerror={() => imageFailed = true} />
  {:else}
    <canvas bind:this={canvas}></canvas>
  {/if}
  <div
    class="shade"
    style:background={showImage
      ? 'linear-gradient(90deg, rgba(0, 0, 0, .76) 0%, rgba(0, 0, 0, .48) 45%, rgba(0, 0, 0, 0) 78%)'
      : `linear-gradient(90deg, ${rgba(th.sceneTop, 0.25)} 0%, ${rgba(th.sceneTop, 0.85)} 50%, ${rgba(th.sceneTop, 0)} 76%)`}
  ></div>
  <div class="text">
    <h1>{title}</h1>
    <p>{subtitle}</p>
  </div>
</div>

<style>
  .banner { position: relative; border-radius: 14px; overflow: hidden; background: var(--scene-top); }
  canvas, .screenshot { position: absolute; inset: 0; width: 100%; height: 100%; }
  canvas { image-rendering: pixelated; }
  .screenshot { object-fit: cover; }
  .shade { position: absolute; inset: 0; }
  .text {
    position: relative; height: 100%; display: flex; flex-direction: column;
    align-items: center; justify-content: center; text-align: center; padding: 0 20px; color: #fff;
    width: 76%;
  }
  .image .text { align-items: flex-start; text-align: left; width: 60%; }
  h1 { margin: 0; font-size: 44px; line-height: 1.05; text-shadow: 0 3px 0 rgba(0, 0, 0, .25); max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  p { margin: 8px 0 0; font-size: 17px; color: #D9CFFA; }
</style>
