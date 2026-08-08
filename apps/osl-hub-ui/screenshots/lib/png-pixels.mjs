import { inflateSync } from "node:zlib";

/**
 * Minimal 8-bit RGB/RGBA non-interlaced PNG reader. Returns RGBA pixels so a
 * screenshot region can be compared against a source picture sample by sample.
 */
export function readPng(buffer) {
  if (buffer.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") {
    throw new Error("not a PNG");
  }
  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const data = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString("ascii");
    const chunk = buffer.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;
    if (type === "IHDR") {
      width = chunk.readUInt32BE(0);
      height = chunk.readUInt32BE(4);
      if (chunk[8] !== 8) throw new Error(`unsupported PNG bit depth ${chunk[8]}`);
      colorType = chunk[9];
      if (colorType !== 2 && colorType !== 6) throw new Error(`unsupported PNG color type ${colorType}`);
      if (chunk[12] !== 0) throw new Error("interlaced PNG is not supported");
    } else if (type === "IDAT") {
      data.push(chunk);
    } else if (type === "IEND") {
      break;
    }
  }
  const channels = colorType === 6 ? 4 : 3;
  const stride = width * channels;
  const inflated = inflateSync(Buffer.concat(data));
  const pixels = Buffer.alloc(width * height * 4);
  let source = 0;
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[source];
    source += 1;
    const row = Buffer.from(inflated.subarray(source, source + stride));
    source += stride;
    for (let x = 0; x < stride; x += 1) {
      const left = x >= channels ? row[x - channels] : 0;
      const up = previous[x] ?? 0;
      const upLeft = x >= channels ? previous[x - channels] : 0;
      if (filter === 1) row[x] = (row[x] + left) & 0xff;
      else if (filter === 2) row[x] = (row[x] + up) & 0xff;
      else if (filter === 3) row[x] = (row[x] + Math.floor((left + up) / 2)) & 0xff;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        row[x] = (row[x] + (pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft)) & 0xff;
      } else if (filter !== 0) {
        throw new Error(`unsupported PNG row filter ${filter}`);
      }
    }
    for (let x = 0; x < width; x += 1) {
      const src = x * channels;
      const dst = (y * width + x) * 4;
      pixels[dst] = row[src];
      pixels[dst + 1] = row[src + 1];
      pixels[dst + 2] = row[src + 2];
      pixels[dst + 3] = channels === 4 ? row[src + 3] : 255;
    }
    previous = row;
  }
  return { width, height, pixels };
}

/**
 * Mean absolute RGB difference between `picture` and the region of `screenshot`
 * whose top-left corner is (`x`, `y`). 0 means the picture is present in the
 * screenshot sample for sample.
 */
export function regionMeanAbsDiff(screenshot, picture, x, y) {
  let total = 0;
  let samples = 0;
  for (let row = 0; row < picture.height; row += 1) {
    for (let column = 0; column < picture.width; column += 1) {
      const shotX = x + column;
      const shotY = y + row;
      if (shotX < 0 || shotY < 0 || shotX >= screenshot.width || shotY >= screenshot.height) {
        throw new Error(`picture region falls outside the screenshot at ${shotX},${shotY}`);
      }
      const shot = (shotY * screenshot.width + shotX) * 4;
      const source = (row * picture.width + column) * 4;
      total += Math.abs(screenshot.pixels[shot] - picture.pixels[source]);
      total += Math.abs(screenshot.pixels[shot + 1] - picture.pixels[source + 1]);
      total += Math.abs(screenshot.pixels[shot + 2] - picture.pixels[source + 2]);
      samples += 3;
    }
  }
  return total / samples;
}
