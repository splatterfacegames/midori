/**
 * Generate the Midori application icon (icons/icon.png) with no dependencies —
 * a flat-design tree mark: dark ground, brown trunk, layered green canopy.
 */

import { deflateSync } from 'node:zlib';
import { writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const SIZE = 512;

// CRC32 for PNG chunks
const table = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = table[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
};

const px = new Uint8Array(SIZE * SIZE * 4);
const put = (x, y, [r, g, b, a]) => {
  const i = (y * SIZE + x) * 4;
  px[i] = r; px[i + 1] = g; px[i + 2] = b; px[i + 3] = a;
};
const disc = (cx, cy, radius, color) => {
  for (let y = Math.max(0, Math.floor(cy - radius)); y <= Math.min(SIZE - 1, Math.ceil(cy + radius)); y++) {
    for (let x = Math.max(0, Math.floor(cx - radius)); x <= Math.min(SIZE - 1, Math.ceil(cx + radius)); x++) {
      if ((x - cx) ** 2 + (y - cy) ** 2 <= radius * radius) put(x, y, color);
    }
  }
};
const fill = (color) => { for (let i = 0; i < SIZE * SIZE; i++) put(i % SIZE, Math.floor(i / SIZE), color); };
const rect = (x0, y0, x1, y1, color) => {
  for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) put(x, y, color);
};

fill([26, 36, 24, 255]);              // ground
rect(240, 256, 272, 448, [138, 107, 70, 255]); // trunk
disc(256, 200, 150, [63, 122, 46, 255]);       // canopy base
disc(150, 250, 95, [76, 138, 61, 255]);
disc(362, 250, 95, [76, 138, 61, 255]);
disc(256, 120, 80, [93, 156, 72, 255]);        // crown highlight

const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // filter: none
  Buffer.from(px.buffer, y * SIZE * 4, SIZE * 4).copy(raw, y * (SIZE * 4 + 1) + 1);
}
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8;  // bit depth
ihdr[9] = 6;  // color type RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', ihdr),
  chunk('IDAT', deflateSync(raw, { level: 9 })),
  chunk('IEND', Buffer.alloc(0)),
]);

const out = resolve(root, 'crates/midori-desktop/icons');
mkdirSync(out, { recursive: true });
writeFileSync(resolve(out, 'icon.png'), png);
console.log(`Wrote ${resolve(out, 'icon.png')} (${png.length} bytes)`);
