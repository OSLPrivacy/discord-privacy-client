/**
 * Minimal PNG reader for screenshot evidence.
 *
 * A capture that only checks byte length proves the browser wrote a file, not
 * that the screen drew anything. These helpers decode the screenshot so a check
 * can say what a named rectangle actually contains: how many distinct colours,
 * how many pixels differ from the page background, and what the mean colour is.
 * That is the difference between "a PNG exists" and "the preview is visible".
 */
import { inflateSync } from "node:zlib";

const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

export function parsePng(buffer) {
  if (!buffer.subarray(0, 8).equals(PNG_SIGNATURE)) throw new Error("screenshot is not a PNG");
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

function isBackground(r, g, b, background) {
  return Math.abs(r - background[0]) + Math.abs(g - background[1]) + Math.abs(b - background[2]) <= 24;
}

/** Facts about one rectangle of the screenshot, in device pixels. */
export function cropFacts(png, rect, { background = [10, 10, 10], brightSum = 360 } = {}) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const colors = new Set();
  let count = 0;
  let saturated = 0;
  let nonBackground = 0;
  let brightPixels = 0;
  let totalR = 0;
  let totalG = 0;
  let totalB = 0;
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const r = png.pixels[offset];
      const g = png.pixels[offset + 1];
      const b = png.pixels[offset + 2];
      colors.add(`${r},${g},${b}`);
      // "Saturated" = a colour no part of the grey app chrome can produce, so a
      // count above zero means real picture pixels, not a card border.
      if (Math.max(r, g, b) - Math.min(r, g, b) > 30) saturated += 1;
      if (!isBackground(r, g, b, background)) nonBackground += 1;
      if (r + g + b > brightSum) brightPixels += 1;
      totalR += r;
      totalG += g;
      totalB += b;
      count += 1;
    }
  }
  return {
    width: x1 - x0,
    height: y1 - y0,
    pixels: count,
    distinctColors: colors.size,
    saturatedPixels: saturated,
    nonBackground,
    brightPixels,
    meanColor: count === 0 ? null : `${Math.round(totalR / count)},${Math.round(totalG / count)},${Math.round(totalB / count)}`,
  };
}

/** Whole-image facts plus the named crops. */
export function imageFacts(buffer, rects = {}, options = {}) {
  const background = options.background ?? [10, 10, 10];
  const png = parsePng(buffer);
  const colors = new Set();
  let nonBackground = 0;
  for (let index = 0; index < png.pixels.length; index += 4) {
    const r = png.pixels[index];
    const g = png.pixels[index + 1];
    const b = png.pixels[index + 2];
    colors.add(`${r},${g},${b}`);
    if (!isBackground(r, g, b, background)) nonBackground += 1;
  }
  return {
    width: png.width,
    height: png.height,
    distinctColors: colors.size,
    nonBackground,
    nearlyBlank: colors.size < 20 || nonBackground < 2_000,
    crops: Object.fromEntries(Object.entries(rects).map(([name, rect]) => [name, cropFacts(png, rect, options)])),
  };
}
