//! Zero-allocation, stack-based JSON output via const-generic fixed buffers.

/// Stack-allocated, const-generic byte buffer for zero-allocation JSON output.
/// `N` is the maximum bytes; exceeding it panics.
pub struct FixedBuf<const N: usize> {
    data: [u8; N],
    len: usize,
}

impl<const N: usize> Default for FixedBuf<N> {
    fn default() -> Self { Self::new() }
}

impl<const N: usize> FixedBuf<N> {
    /// Construct an empty `FixedBuf` (const-fn, lives on the stack).
    pub const fn new() -> Self {
        FixedBuf { data: [0u8; N], len: 0 }
    }

    #[inline] pub fn as_slice(&self) -> &[u8] { &self.data[..self.len] }

    /// # Panics
    /// If content is not valid UTF-8 (never happens for correct `ToJson` impls).
    #[inline]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.as_slice()).expect("ToJson always emits valid UTF-8")
    }

    #[inline] pub fn len(&self) -> usize { self.len }
    #[inline] pub fn is_empty(&self) -> bool { self.len == 0 }
    #[inline] pub fn remaining(&self) -> usize { N - self.len }
    #[inline] pub fn clear(&mut self) { self.len = 0; }
}

impl<const N: usize> FixedBuf<N> {
    #[inline(always)]
    fn write_bytes(&mut self, bs: &[u8]) {
        let end = self.len + bs.len();
        self.data[self.len..end].copy_from_slice(bs);
        self.len = end;
    }
}

impl<const N: usize> core::fmt::Debug for FixedBuf<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("FixedBuf").field(&self.as_str()).finish()
    }
}

use crate::ser::ToJson;

impl<T: ToJson + ?Sized> ToJsonExt for T {}

/// Extension trait adding stack-buffer and reuse helpers for every `T: ToJson`.
pub trait ToJsonExt: ToJson {
    /// Serialize into a stack-allocated [`FixedBuf<N>`]. Returns `None` if output exceeds `N`.
    #[inline]
    fn to_fixed_buf<const N: usize>(&self) -> Option<FixedBuf<N>> {
        let mut tmp = Vec::with_capacity(N);
        self.json_write(&mut tmp);
        if tmp.len() > N { return None; }
        let mut buf = FixedBuf::<N>::new();
        buf.write_bytes(&tmp);
        Some(buf)
    }

    /// Serialize to a pre-allocated `Vec<u8>`, clearing it first (amortizes allocation).
    #[inline]
    fn json_write_reuse<'a>(&self, buf: &'a mut Vec<u8>) -> &'a [u8] {
        buf.clear();
        self.json_write(buf);
        buf.as_slice()
    }

    /// Serialize to any `io::Write`.
    fn json_write_io(&self, mut w: impl std::io::Write) -> std::io::Result<()> {
        w.write_all(&self.to_json_bytes())
    }
}

/// Compute the exact serialized length of a JSON string (quotes + escapes) at compile time.
pub const fn json_str_len(s: &[u8]) -> usize {
    let mut len = 2; // surrounding quotes
    let mut i = 0;
    while i < s.len() {
        len += match s[i] {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 0x08 | 0x0C => 2,
            0x00..=0x1F => 6, // \uXXXX
            _ => 1,
        };
        i += 1;
    }
    len
}
