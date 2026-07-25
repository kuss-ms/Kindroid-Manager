// Generate correct minimal 1x1 RGBA PNG, wrap into ICO and ICNS.
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

function crc32(buf) {
  let c,
    table = [];
  for (let n = 0; n < 256; n++) {
    c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  let crc = 0xffffffff;
  for (let i = 0; i < buf.length; i++) crc = table[(crc ^ buf[i]) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, 'ascii');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crc]);
}

function makePng(width, height) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.writeUInt8(8, 8); // bit depth
  ihdr.writeUInt8(6, 9); // RGBA
  ihdr.writeUInt8(0, 10); // compression
  ihdr.writeUInt8(0, 11); // filter
  ihdr.writeUInt8(0, 12); // interlace
  // raw pixel: filter byte 0 + RGBA (0,0,0,0)
  const raw = Buffer.concat([Buffer.from([0]), Buffer.from([0, 0, 0, 0])]);
  const idat = zlib.deflateSync(raw);
  return Buffer.concat([
    sig,
    chunk('IHDR', ihdr),
    chunk('IDAT', idat),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const outDir = path.resolve(__dirname, '..', 'src-tauri', 'icons');
fs.mkdirSync(outDir, { recursive: true });
const PNG_1x1 = makePng(1, 1);

for (const name of ['32x32.png', '128x128.png', '128x128@2x.png']) {
  fs.writeFileSync(path.join(outDir, name), PNG_1x1);
}

const dir = Buffer.alloc(6);
dir.writeUInt16LE(0, 0);
dir.writeUInt16LE(1, 2);
dir.writeUInt16LE(1, 4);
const entry = Buffer.alloc(16);
entry.writeUInt8(1, 0);
entry.writeUInt8(1, 1);
entry.writeUInt8(0, 2);
entry.writeUInt8(0, 3);
entry.writeUInt16LE(1, 4);
entry.writeUInt16LE(32, 6);
entry.writeUInt32LE(PNG_1x1.length, 8);
entry.writeUInt32LE(6 + 16, 12);
fs.writeFileSync(path.join(outDir, 'icon.ico'), Buffer.concat([dir, entry, PNG_1x1]));

const icnsHeader = Buffer.alloc(8);
icnsHeader.write('icns', 0, 'ascii');
const ic07 = Buffer.alloc(8);
ic07.write('ic07', 0, 'ascii');
ic07.writeUInt32BE(8 + PNG_1x1.length, 4);
icnsHeader.writeUInt32BE(8 + 8 + PNG_1x1.length, 4);
fs.writeFileSync(path.join(outDir, 'icon.icns'), Buffer.concat([icnsHeader, ic07, PNG_1x1]));

console.log('Wrote proper icons in', outDir);
