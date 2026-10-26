// Draws the pixel-art hut icon (16x16 grid) and writes src-tauri/icons/source.png (1024px).
// Then run: npx tauri icon src-tauri/icons/source.png
import sharp from "sharp";
import { writeFileSync, mkdirSync } from "node:fs";

const art = [
  "................",
  "................",
  ".......CC.......",
  "......CCCC......",
  ".....CCCCCC.....",
  "....CCCCCCCC....",
  "...CCCCCCCCCC...",
  "..CCCCCCCCCCCC..",
  ".kkkkkkkkkkkkkk.",
  "...YYYYYYYYYY...",
  ".s.YYYYDDYYYY.s.",
  "sssYYYYDDYYYYsss",
  "sSsYYYYDDYYYYsSs",
  "................",
  "................",
  "................",
];
const color = { C: "#f6f1d9", k: "#d6cda6", Y: "#8fdcf0", D: "#0a3d7a", s: "#5cc0e0", S: "#3aa3d0" };
const px = 52;
const pad = (1024 - 16 * px) / 2;
let rects = "";
art.forEach((row, y) =>
  [...row].forEach((c, x) => {
    if (color[c]) rects += `<rect x="${pad + x * px}" y="${pad + (y + 1.5) * px}" width="${px}" height="${px}" fill="${color[c]}"/>`;
  }),
);
const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2a7fd4"/><stop offset="1" stop-color="#0b4f9c"/></linearGradient></defs>
<rect x="32" y="32" width="960" height="960" rx="216" fill="url(#g)"/>
<g shape-rendering="crispEdges">${rects}</g></svg>`;
mkdirSync("src-tauri/icons", { recursive: true });
writeFileSync("src-tauri/icons/source.svg", svg);
await sharp(Buffer.from(svg)).png().toFile("src-tauri/icons/source.png");
console.log("icon written");
