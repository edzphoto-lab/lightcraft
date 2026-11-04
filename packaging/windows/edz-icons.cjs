// Build-time recolouring of the original MIT/Apache-2.0 lynx; no changes to its artwork.
const fs = require('node:fs');
const path = require('node:path');
const { Resvg } = require('@resvg/resvg-js');
const root = path.resolve(__dirname, '../..');
const iconDir = path.join(root, 'assets/app-icon');
const original = fs.readFileSync(path.join(iconDir, 'lightcraft.svg'), 'utf8');
if ((original.match(/fill="#f2a516"/g) || []).length !== 1) {
  throw new Error('Expected exactly one yellow background in the original lynx icon');
}
const svg = original.replace('fill="#f2a516"', 'fill="#c42032"');
const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const pngs = sizes.map(width => new Resvg(svg, { fitTo: { mode: 'width', value: width } }).render().asPng());
const header = Buffer.alloc(6 + 16 * sizes.length);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(sizes.length, 4);
let offset = header.length;
sizes.forEach((size, i) => {
  const entry = 6 + i * 16;
  header[entry] = size === 256 ? 0 : size;
  header[entry + 1] = size === 256 ? 0 : size;
  header.writeUInt16LE(1, entry + 4);
  header.writeUInt16LE(32, entry + 6);
  header.writeUInt32LE(pngs[i].length, entry + 8);
  header.writeUInt32LE(offset, entry + 12);
  offset += pngs[i].length;
});
fs.writeFileSync(path.join(iconDir, 'lightcraft.ico'), Buffer.concat([header, ...pngs]));
// The Windows viewport embeds this PNG; changing just the .ico would leave the taskbar wrong.
fs.writeFileSync(path.join(iconDir, 'hicolor/256x256/apps/ai.storyteller.lightcraft.png'), pngs[pngs.length - 1]);
console.log('Prepared red EdZ icon: PE/Explorer, installer, shortcuts and running window.');
