//! Panic-free length-checked binary reader/writer and LEB128 varints.
//!
//! Everything that touches attacker-controlled bytes goes through
//! [`Reader`]. It never indexes a slice directly and never panics; a
//! short buffer produces [`Error::Malformed`].
//!
//! ## Why varints
//!
//! The carrier is low-bandwidth cover text and every wire byte costs
//! roughly 4/3 bytes of base64 before the steganographic layer
//! expands it further. Message counters, chain lengths and PQ epoch
//! numbers are almost always small, so LEB128 encodes each of the
//! four header counters in one byte instead of four. That is ~12
//! bytes saved on every single message versus fixed `u32`s.
//!
//! LEB128 here is **canonical**: the decoder rejects encodings with
//! redundant leading zero groups and encodings longer than 5 groups.
//! Non-canonical acceptance would make the header malleable, and the
//! header is covered by an AEAD tag whose associated data includes
//! the exact header bytes — so malleability there would be a
//! correctness hazard even though it is not directly forgeable.

use crate::error::{Error, Result};

/// Maximum LEB128 groups for a `u32` (5 × 7 = 35 ≥ 32 bits).
const MAX_VARINT_GROUPS: usize = 5;

/// Growable little-endian writer. Infallible; capacity is the only
/// resource and callers bound the sizes they write.
#[derive(Default, Clone)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn with_capacity(n: usize) -> Self {
        Writer {
            buf: Vec::with_capacity(n),
        }
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(v);
        self
    }

    /// Canonical LEB128.
    pub fn varint(&mut self, mut v: u32) -> &mut Self {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(byte);
                return self;
            }
            self.buf.push(byte | 0x80);
        }
    }

    /// Length-prefixed byte string (varint length).
    pub fn var_bytes(&mut self, v: &[u8]) -> Result<&mut Self> {
        let len = u32::try_from(v.len()).map_err(|_| Error::PolicyBound("var_bytes too long"))?;
        self.varint(len);
        self.buf.extend_from_slice(v);
        Ok(self)
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }
}

/// Bounds-checked reader over borrowed bytes.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Reject trailing bytes. Called at the end of every parse so a
    /// hostile peer cannot append an unauthenticated tail.
    pub fn finish(self) -> Result<()> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(Error::Malformed("trailing bytes"))
        }
    }

    pub fn u8(&mut self) -> Result<u8> {
        let b = *self
            .buf
            .get(self.pos)
            .ok_or(Error::Malformed("short read: u8"))?;
        self.pos += 1;
        Ok(b)
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(Error::Malformed("length overflow"))?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(Error::Malformed("short read: slice"))?;
        self.pos = end;
        Ok(slice)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let slice = self.take(N)?;
        let mut out = [0u8; N];
        // `slice.len() == N` is guaranteed by `take`.
        out.copy_from_slice(slice);
        Ok(out)
    }

    /// Canonical LEB128 decode. Rejects over-long and non-canonical
    /// encodings.
    pub fn varint(&mut self) -> Result<u32> {
        // Accumulate in u64 so the final group cannot shift-overflow;
        // the `> u32::MAX` check then rejects out-of-range values.
        let mut result: u64 = 0;
        for group in 0..MAX_VARINT_GROUPS {
            let byte = self.u8()?;
            let payload = u64::from(byte & 0x7f);
            result |= payload << (group * 7);
            if result > u64::from(u32::MAX) {
                return Err(Error::Malformed("varint overflow"));
            }
            if byte & 0x80 == 0 {
                // Canonicality: a final group of zero after at least
                // one continuation carries no information.
                if group > 0 && payload == 0 {
                    return Err(Error::Malformed("non-canonical varint"));
                }
                return Ok(result as u32);
            }
        }
        Err(Error::Malformed("varint too long"))
    }

    pub fn var_bytes(&mut self) -> Result<&'a [u8]> {
        let len = self.varint()? as usize;
        self.take(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: u32) {
        let mut w = Writer::default();
        w.varint(v);
        let bytes = w.into_vec();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.varint().expect("decode"), v, "roundtrip {v}");
        assert!(r.is_empty());
    }

    #[test]
    fn varint_roundtrips_boundaries() {
        for v in [
            0u32,
            1,
            126,
            127,
            128,
            255,
            16_383,
            16_384,
            u32::from(u16::MAX),
            1 << 21,
            (1 << 28) - 1,
            1 << 28,
            u32::MAX - 1,
            u32::MAX,
        ] {
            roundtrip(v);
        }
    }

    #[test]
    fn varint_rejects_overlong() {
        // Six continuation groups.
        let bytes = [0x80u8, 0x80, 0x80, 0x80, 0x80, 0x00];
        let mut r = Reader::new(&bytes);
        assert!(r.varint().is_err());
    }

    #[test]
    fn varint_rejects_non_canonical_zero_tail() {
        let bytes = [0x80u8, 0x00];
        let mut r = Reader::new(&bytes);
        assert!(r.varint().is_err());
    }

    #[test]
    fn varint_rejects_bit_overflow() {
        // 5 groups where the top group sets bits above 2^32.
        let bytes = [0xffu8, 0xff, 0xff, 0xff, 0x7f];
        let mut r = Reader::new(&bytes);
        assert!(r.varint().is_err());
    }

    #[test]
    fn reader_never_panics_on_truncation() {
        for len in 0..40usize {
            let buf = vec![0xffu8; len];
            let mut r = Reader::new(&buf);
            let _ = r.u8();
            let _ = r.varint();
            let _ = r.array::<32>();
            let _ = r.var_bytes();
            let _ = r.take(usize::MAX);
        }
    }
}
