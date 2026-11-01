// Expressive "cookie" / clover shapes as CSS clip-path polygons:
// radius(θ) = r · (1 + amp·cos(n·θ))
const cache = new Map();

export function cookie(n = 8, amp = 0.07, rot = 0, steps = 96) {
  const key = `${n}|${amp}|${rot}`;
  if (cache.has(key)) return cache.get(key);
  const norm = 1 + amp;
  const pts = [];
  for (let i = 0; i < steps; i++) {
    const a = (i * 2 * Math.PI) / steps;
    const r = ((1 + amp * Math.cos(n * a)) / norm) * 50;
    pts.push(`${(50 + r * Math.cos(a + rot)).toFixed(2)}% ${(50 + r * Math.sin(a + rot)).toFixed(2)}%`);
  }
  const v = `polygon(${pts.join(',')})`;
  cache.set(key, v);
  return v;
}

/** soft 4-lobed clover */
export const clover = () => cookie(4, 0.14, Math.PI / 4);
