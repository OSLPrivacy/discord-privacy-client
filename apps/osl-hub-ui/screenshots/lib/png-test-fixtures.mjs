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

/** A valid, opaque RGBA PNG with exactly the requested dimensions. */
export function rgbaPng(width, height, pixelAt, { compressionLevel = 6 } = {}) {
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
      const colour = pixelAt(x, y);
      if (typeof colour === "number") {
        rows[pixel] = colour >>> 16;
        rows[pixel + 1] = (colour >>> 8) & 0xff;
        rows[pixel + 2] = colour & 0xff;
      } else {
        rows[pixel] = colour[0];
        rows[pixel + 1] = colour[1];
        rows[pixel + 2] = colour[2];
      }
      rows[pixel + 3] = 255;
    }
  }
  return Buffer.concat([
    Buffer.from("89504e470d0a1a0a", "hex"),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(rows, { level: compressionLevel })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

/** A valid, opaque, one-colour RGBA PNG with exactly the requested dimensions. */
export function blankRgbaPng(width, height, colour = [255, 255, 255], options) {
  return rgbaPng(width, height, () => colour, options);
}
