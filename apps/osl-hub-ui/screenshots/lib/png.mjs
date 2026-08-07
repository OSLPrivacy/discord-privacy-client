/**
 * Just enough PNG to read what Chrome actually painted: decode the screenshot
 * back to pixels, then ask whether a given box on the page has ink in it.
 *
 * A capture that only checks the DOM proves the words exist, not that they were
 * drawn; every screenshot finish line in this repo is about the image.
 */

import { inflateSync } from "node:zlib";

const SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

export function parsePng(buffer) {
  if (!buffer.subarray(0, 8).equals(SIGNATURE)) throw new Error("screenshot is not a PNG");
  let offset = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = 0;
  const idat = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.toString("ascii", offset + 4, offset + 8);
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === "IDAT") {
      idat.push(data);
    } else if (type === "IEND") {
      break;
    }
  }
  if (bitDepth !== 8 || ![2, 6].includes(colorType)) {
    throw new Error(`unsupported PNG encoding bitDepth=${bitDepth} colorType=${colorType}`);
  }
  const channels = colorType === 6 ? 4 : 3;
  const stride = width * channels;
  const inflated = inflateSync(Buffer.concat(idat));
  const pixels = Buffer.alloc(width * height * 4);
  let inOffset = 0;
  const prior = Buffer.alloc(stride);
  const row = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[inOffset];
    inOffset += 1;
    for (let x = 0; x < stride; x += 1) {
      const raw = inflated[inOffset + x];
      const left = x >= channels ? row[x - channels] : 0;
      const up = prior[x];
      const upLeft = x >= channels ? prior[x - channels] : 0;
      let value;
      if (filter === 0) value = raw;
      else if (filter === 1) value = raw + left;
      else if (filter === 2) value = raw + up;
      else if (filter === 3) value = raw + Math.floor((left + up) / 2);
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        value = raw + (pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft);
      } else {
        throw new Error(`unsupported PNG filter ${filter}`);
      }
      row[x] = value & 0xff;
    }
    inOffset += stride;
    for (let x = 0; x < width; x += 1) {
      const source = x * channels;
      const target = (y * width + x) * 4;
      pixels[target] = row[source];
      pixels[target + 1] = row[source + 1];
      pixels[target + 2] = row[source + 2];
      pixels[target + 3] = channels === 4 ? row[source + 3] : 255;
    }
    row.copy(prior);
  }
  return { width, height, pixels };
}

/**
 * "Ink" = pixels that differ from the most common colour in the box. Text drawn
 * muted-on-panel is low contrast, so counting bright pixels alone would call a
 * painted line blank.
 */
export function inkFacts(png, rect) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const counts = new Map();
  const colors = [];
  let sumR = 0;
  let sumG = 0;
  let sumB = 0;
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const r = png.pixels[offset];
      const g = png.pixels[offset + 1];
      const b = png.pixels[offset + 2];
      sumR += r;
      sumG += g;
      sumB += b;
      const key = `${r},${g},${b}`;
      counts.set(key, (counts.get(key) ?? 0) + 1);
      colors.push(key);
    }
  }
  let background = "";
  let best = -1;
  for (const [key, count] of counts) {
    if (count > best) {
      best = count;
      background = key;
    }
  }
  const [br, bg, bb] = background.split(",").map(Number);
  let ink = 0;
  for (const key of colors) {
    const [r, g, b] = key.split(",").map(Number);
    if (Math.abs(r - br) + Math.abs(g - bg) + Math.abs(b - bb) > 24) ink += 1;
  }
  const pixelCount = Math.max(1, colors.length);
  return {
    width: x1 - x0,
    height: y1 - y0,
    distinctColors: counts.size,
    ink,
    mean: [Math.round(sumR / pixelCount), Math.round(sumG / pixelCount), Math.round(sumB / pixelCount)],
    insidePng: x0 >= 0 && y0 >= 0 && x1 <= png.width && y1 <= png.height && x1 > x0 && y1 > y0,
  };
}

/** How many pixels two same-sized screenshots disagree about. */
export function pixelDifference(left, right) {
  if (left.width !== right.width || left.height !== right.height) {
    throw new Error(`screenshots are different sizes: ${left.width}x${left.height} vs ${right.width}x${right.height}`);
  }
  let differing = 0;
  for (let index = 0; index < left.pixels.length; index += 4) {
    if (
      left.pixels[index] !== right.pixels[index]
      || left.pixels[index + 1] !== right.pixels[index + 1]
      || left.pixels[index + 2] !== right.pixels[index + 2]
    ) differing += 1;
  }
  const total = left.width * left.height;
  return { differing, total, percent: Number(((differing / total) * 100).toFixed(2)) };
}

export function meanDistance(left, right) {
  return Math.abs(left[0] - right[0]) + Math.abs(left[1] - right[1]) + Math.abs(left[2] - right[2]);
}
