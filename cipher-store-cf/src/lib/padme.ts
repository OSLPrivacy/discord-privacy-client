/**
 * Return the Padmé-padded length for a positive, safely representable length.
 *
 * Padmé (PoPETs 2019.4) keeps the leading `S = floor(log2 E) + 1` bits of
 * `L`, where `E = floor(log2 L)`, and clears the rest by rounding upward.
 */
export function padme(length: number): number {
  if (!Number.isSafeInteger(length) || length <= 0) {
    throw new RangeError("Padmé length must be a positive safe integer");
  }

  const exponent = Math.floor(Math.log2(length));
  if (exponent === 0) return length;

  const significantBits = Math.floor(Math.log2(exponent)) + 1;
  const granularity = 2 ** (exponent - significantBits);
  const padded = Math.ceil(length / granularity) * granularity;
  if (!Number.isSafeInteger(padded)) {
    throw new RangeError("Padmé padded length exceeds the safe integer range");
  }
  return padded;
}

/** Whether a stored object length is already a canonical Padmé length. */
export function isPadmeLength(length: number): boolean {
  return Number.isSafeInteger(length) && length > 0 && padme(length) === length;
}
