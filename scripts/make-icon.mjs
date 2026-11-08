// Generates a 1024x1024 RGBA app icon (rounded square + 2x2 tile grid)
// using only Node builtins, then lets `tauri icon` derive the .ico/.icns set.
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";

const SIZE = 1024;
const SS = 3; // supersampling factor per axis

const BG = [14, 18, 26]; // #0e121a
const TILES = [
  [0.255, 0.255, [111, 155, 255]], // top-left  blue
  [0.505, 0.255, [77, 212, 172]], // top-right emerald
  [0.255, 0.505, [139, 156, 255]], // bottom-left periwinkle
  [0.505, 0.505, [226, 232, 255]], // bottom-right near-white
];
const TILE = 0.24;
const TILE_R = 0.075;
const BG_R = 0.215;

function insideRounded(x, y, x0, y0, w, h, r) {
  if (x < x0 || y < y0 || x > x0 + w || y > y0 + h) return false;
  const cx = Math.min(Math.max(x, x0 + r), x0 + w - r);
  const cy = Math.min(Math.max(y, y0 + r), y0 + h - r);
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= r * r;
}

const out = Buffer.alloc(SIZE * SIZE * 4);

for (let py = 0; py < SIZE; py++) {
  for (let px = 0; px < SIZE; px++) {
    let r = 0;
    let g = 0;
    let b = 0;
    let a = 0;

    for (let sy = 0; sy < SS; sy++) {
      for (let sx = 0; sx < SS; sx++) {
        const x = (px + (sx + 0.5) / SS) / SIZE;
        const y = (py + (sy + 0.5) / SS) / SIZE;

        if (!insideRounded(x, y, 0, 0, 1, 1, BG_R)) continue;

        // Subtle vertical gradient on the background.
        const t = y;
        let cr = Math.round(BG[0] + (26 - BG[0]) * (1 - t));
        let cg = Math.round(BG[1] + (33 - BG[1]) * (1 - t));
        let cb = Math.round(BG[2] + (46 - BG[2]) * (1 - t));

        for (const [tx, ty, color] of TILES) {
          if (insideRounded(x, y, tx, ty, TILE, TILE, TILE_R)) {
            cr = color[0];
            cg = color[1];
            cb = color[2];
            break;
          }
        }

        r += cr;
        g += cg;
        b += cb;
        a += 255;
      }
    }

    const n = SS * SS;
    const i = (py * SIZE + px) * 4;
    if (a > 0) {
      out[i] = Math.round(r / (a / 255));
      out[i + 1] = Math.round(g / (a / 255));
      out[i + 2] = Math.round(b / (a / 255));
      out[i + 3] = Math.round(a / n);
    }
  }
}

// ---- minimal PNG writer ----
const crcTable = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type RGBA
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0;
  out.copy(raw, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
}

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

mkdirSync("assets", { recursive: true });
writeFileSync("assets/icon-source.png", png);
console.log("wrote assets/icon-source.png", png.length, "bytes");
