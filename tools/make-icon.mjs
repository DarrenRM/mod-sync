// App icon: a square cut from the banner (src/assets/sky.webp), the right-hand cloud with some sky
// and a falling streak, in a rounded tile. Writes src-tauri/icons/source.png (1024px) and the
// landing page icon (worker/public/icon.png). Then run: npx tauri icon src-tauri/icons/source.png
import sharp from "sharp";
import { mkdirSync } from "node:fs";

const CROP = { left: 650, top: 420, width: 430, height: 430 };
const SIZE = 1024;
const PAD = 32; // transparent margin, so the tile sits like other Windows icons
const tile = SIZE - PAD * 2;

const art = await sharp("src/assets/sky.webp").extract(CROP).resize(tile, tile, { kernel: "lanczos3" }).png().toBuffer();
const mask = Buffer.from(`<svg width="${tile}" height="${tile}"><rect width="${tile}" height="${tile}" rx="${Math.round(tile * 0.2)}" fill="#fff"/></svg>`);
const rounded = await sharp(art).composite([{ input: mask, blend: "dest-in" }]).png().toBuffer();

mkdirSync("src-tauri/icons", { recursive: true });
const icon = await sharp({ create: { width: SIZE, height: SIZE, channels: 4, background: { r: 0, g: 0, b: 0, alpha: 0 } } })
  .composite([{ input: rounded, left: PAD, top: PAD }])
  .png()
  .toBuffer();
await sharp(icon).toFile("src-tauri/icons/source.png");
await sharp(icon).resize(256, 256, { kernel: "lanczos3" }).toFile("worker/public/icon.png");
console.log("icon written: src-tauri/icons/source.png, worker/public/icon.png");
