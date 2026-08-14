import { deflateSync } from "node:zlib";

function pngChunk(type, body) {
  const chunk = Buffer.alloc(12 + body.length);
  chunk.writeUInt32BE(body.length, 0);
  chunk.write(type, 4, 4, "ascii");
  body.copy(chunk, 8);
  // readPng deliberately does not need CRCs, but a blank control should still
  // be a structurally valid PNG for any consumer that does validate them.
  let crc = 0xffffffff;
  for (const byte of chunk.subarray(4, 8 + body.length)) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  chunk.writeUInt32BE((crc ^ 0xffffffff) >>> 0, 8 + body.length);
  return chunk;
}

/** A valid, opaque, one-colour RGBA PNG with exactly the requested dimensions. */
export function blankRgbaPng(width, height, [red, green, blue] = [255, 255, 255]) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;

  const rows = Buffer.alloc(height * (1 + width * 4));
  for (let y = 0; y < height; y += 1) {
    const row = y * (1 + width * 4);
    for (let x = 0; x < width; x += 1) {
      const pixel = row + 1 + x * 4;
      rows[pixel] = red;
      rows[pixel + 1] = green;
      rows[pixel + 2] = blue;
      rows[pixel + 3] = 255;
    }
  }
  return Buffer.concat([
    Buffer.from("89504e470d0a1a0a", "hex"),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(rows)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}
