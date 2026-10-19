// Tiny PNG encoder and a generated mod icon: a two-tone gradient tile with a simple white glyph
// per kind of mod (tractor, trailer, building, script…). No fonts, no copyrighted art.
//   iconPng("tractor", 210)  -> Buffer (256×256 RGBA PNG)
import zlib from "node:zlib";

const SIZE = 256;

/**
 * Encode RGBA pixels as a PNG.
 * @param {number} w width
 * @param {number} h height
 * @param {Uint8Array} rgba w*h*4 bytes
 * @returns {Buffer}
 */
export function encodePng(w, h, rgba) {
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0; // filter: none
    Buffer.from(rgba.buffer, rgba.byteOffset + y * w * 4, w * 4).copy(raw, y * (w * 4 + 1) + 1);
  }
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(zlib.crc32(td) >>> 0);
    return Buffer.concat([len, td, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** HSL (h 0–360, s/l 0–1) to [r, g, b] 0–255. */
function hsl(h, s, l) {
  const k = (n) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return [f(0), f(8), f(4)].map((v) => Math.round(v * 255));
}

// Signed-distance shapes in a 0–1 unit square (negative = inside).
const circle = (cx, cy, r) => (x, y) => Math.hypot(x - cx, y - cy) - r;
const ring = (cx, cy, r, t) => (x, y) => Math.abs(Math.hypot(x - cx, y - cy) - r) - t;
const box = (x0, y0, x1, y1, rad = 0) => (x, y) => {
  const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
  const hx = (x1 - x0) / 2 - rad, hy = (y1 - y0) / 2 - rad;
  const dx = Math.abs(x - cx) - hx, dy = Math.abs(y - cy) - hy;
  return Math.hypot(Math.max(dx, 0), Math.max(dy, 0)) + Math.min(Math.max(dx, dy), 0) - rad;
};
const tri = (ax, ay, bx, by, cx, cy) => (x, y) => {
  // Inside test with a soft-ish edge distance (good enough at this size).
  const e = (px, py, qx, qy) => ((qx - px) * (y - py) - (qy - py) * (x - px)) / Math.hypot(qx - px, qy - py);
  return Math.max(e(ax, ay, bx, by), e(bx, by, cx, cy), e(cx, cy, ax, ay));
};
const union = (...fs) => (x, y) => Math.min(...fs.map((f) => f(x, y)));
const minus = (a, b) => (x, y) => Math.max(a(x, y), -b(x, y));

/** The glyph for each kind of mod. */
const GLYPHS = {
  tractor: union(box(0.3, 0.3, 0.56, 0.56, 0.03), box(0.3, 0.5, 0.78, 0.62, 0.03), ring(0.36, 0.68, 0.13, 0.035), ring(0.72, 0.72, 0.08, 0.03), box(0.66, 0.36, 0.69, 0.5)),
  harvester: union(box(0.32, 0.3, 0.8, 0.6, 0.04), box(0.14, 0.52, 0.34, 0.58), ring(0.62, 0.7, 0.11, 0.035), box(0.4, 0.22, 0.58, 0.3, 0.02)),
  implement: union(box(0.2, 0.36, 0.8, 0.44, 0.02), ...[0.24, 0.36, 0.48, 0.6, 0.72].map((x) => box(x, 0.44, x + 0.04, 0.68, 0.015))),
  trailer: union(box(0.18, 0.32, 0.82, 0.6, 0.03), ring(0.4, 0.68, 0.09, 0.03), ring(0.62, 0.68, 0.09, 0.03)),
  loader: union(box(0.22, 0.42, 0.58, 0.62, 0.03), box(0.5, 0.3, 0.82, 0.36, 0.015), box(0.76, 0.3, 0.84, 0.5, 0.015), ring(0.32, 0.7, 0.09, 0.03), ring(0.54, 0.7, 0.09, 0.03)),
  vehicle: union(box(0.16, 0.44, 0.84, 0.62, 0.05), box(0.3, 0.32, 0.62, 0.46, 0.05), circle(0.32, 0.66, 0.08), circle(0.68, 0.66, 0.08)),
  building: union(minus(box(0.24, 0.46, 0.76, 0.78), box(0.44, 0.6, 0.56, 0.78)), tri(0.18, 0.48, 0.5, 0.2, 0.82, 0.48)),
  storage: union(box(0.3, 0.32, 0.5, 0.8, 0.02), box(0.54, 0.4, 0.72, 0.8, 0.02), circle(0.4, 0.32, 0.1), circle(0.63, 0.4, 0.09)),
  decoration: union(...[0.24, 0.42, 0.6, 0.78].map((x) => box(x - 0.025, 0.3, x + 0.025, 0.76, 0.01)), box(0.18, 0.4, 0.84, 0.45), box(0.18, 0.6, 0.84, 0.65)),
  object: union(box(0.22, 0.6, 0.78, 0.68, 0.01), box(0.26, 0.34, 0.74, 0.58, 0.02), box(0.24, 0.7, 0.32, 0.76), box(0.46, 0.7, 0.54, 0.76), box(0.68, 0.7, 0.76, 0.76)),
  script: minus(union(circle(0.5, 0.5, 0.22), ...[0, 1, 2, 3, 4, 5, 6, 7].map((i) => {
    const a = (i * Math.PI) / 4, cx = 0.5 + Math.cos(a) * 0.24, cy = 0.5 + Math.sin(a) * 0.24;
    return circle(cx, cy, 0.06);
  })), circle(0.5, 0.5, 0.09)),
  texture: minus(circle(0.5, 0.5, 0.28), union(circle(0.4, 0.4, 0.05), circle(0.58, 0.36, 0.05), circle(0.66, 0.52, 0.05), circle(0.5, 0.66, 0.08))),
  map: union(tri(0.14, 0.76, 0.42, 0.3, 0.7, 0.76), tri(0.44, 0.76, 0.64, 0.44, 0.86, 0.76), circle(0.72, 0.28, 0.07)),
};

export const GLYPH_KINDS = Object.keys(GLYPHS);

const cache = new Map();

/**
 * A mod icon (256×256), or a wide catalog image when given a size: the glyph sits centred at
 * three quarters of the image's height.
 * @param {keyof typeof GLYPHS} kind the glyph
 * @param {number} hue the tile's base hue (0–360)
 * @param {number} [w] width
 * @param {number} [h] height
 * @returns {Buffer} PNG bytes
 */
export function iconPng(kind, hue, w = SIZE, h = SIZE) {
  const key = `${kind}:${Math.round(hue)}:${w}x${h}`;
  if (cache.has(key)) return cache.get(key);
  const glyph = GLYPHS[kind] ?? GLYPHS.script;
  const top = hsl(hue, 0.42, 0.46);
  const bottom = hsl((hue + 24) % 360, 0.5, 0.24);
  const px = new Uint8Array(w * h * 4);
  const sub = [0.25, 0.75];
  // Square images fill with the glyph; wide ones keep it at 75% of the height, centred.
  const scale = w === h ? h : h * 0.75;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const t = (x / w + y / h) / 2;
      let cover = 0;
      for (const sy of sub) for (const sx of sub) if (glyph((x + sx - w / 2) / scale + 0.5, (y + sy - h / 2) / scale + 0.5) <= 0) cover++;
      const a = (cover / 4) * 0.92;
      const i = (y * w + x) * 4;
      for (let c = 0; c < 3; c++) {
        const bg = top[c] * (1 - t) + bottom[c] * t;
        px[i + c] = Math.round(bg * (1 - a) + 250 * a);
      }
      px[i + 3] = 255;
    }
  }
  const png = encodePng(w, h, px);
  cache.set(key, png);
  return png;
}
