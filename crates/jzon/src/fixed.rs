//! Zero-allocation, stack-based JSON output via const-generic fixed buffers.

use crate::ser::{IoSink, LengthCounter, ToJson};

/// Stack-allocated, const-generic byte buffer for zero-allocation JSON output.
pub struct FixedBuf<const N: usize> {
    data: [u8; N],
    len: usize,
    overflow: bool,
}

impl<const N: usize> Default for FixedBuf<N> {
    fn default() -> Self { Self::new() }
}

impl<const N: usize> FixedBuf<N> {
    /// Construct an empty `FixedBuf` (const-fn, lives on the stack).
    pub const fn new() -> Self {
        FixedBuf { data: [0u8; N], len: 0, overflow: false }
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
    #[inline] pub fn clear(&mut self) { self.len = 0; self.overflow = false; }
}

impl<const N: usize> core::fmt::Debug for FixedBuf<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("FixedBuf").field(&self.as_str()).finish()
    }
}

impl<const N: usize> FixedBuf<N> {
    #[inline]
    pub(crate) fn sink_push(&mut self, b: u8) {
        if self.overflow {
            return;
        }
        if self.len >= N {
            self.overflow = true;
            return;
        }
        self.data[self.len] = b;
        self.len += 1;
    }

    #[inline]
    pub(crate) fn sink_extend(&mut self, bs: &[u8]) {
        if self.overflow {
            return;
        }
        let end = self.len + bs.len();
        if end > N {
            self.overflow = true;
            return;
        }
        self.data[self.len..end].copy_from_slice(bs);
        self.len = end;
    }

    #[inline]
    pub(crate) fn sink_ok(&self) -> bool {
        !self.overflow
    }
}

/// Extension trait adding stack-buffer and reuse helpers for every `T: ToJson`.
pub trait ToJsonExt: ToJson {
    /// Serialize into a stack-allocated [`FixedBuf<N>`]. Returns `None` if output exceeds `N`.
    #[inline]
    fn to_fixed_buf<const N: usize>(&self) -> Option<FixedBuf<N>>
    where
        Self: Sized,
    {
        let mut buf = FixedBuf::<N>::new();
        self.json_write_sink(&mut buf);
        if buf.sink_ok() { Some(buf) } else { None }
    }

    /// Exact serialized byte length without allocating output storage.
    #[inline]
    fn json_byte_len(&self) -> usize
    where
        Self: Sized,
    {
        let mut counter = LengthCounter::new();
        self.json_write_sink(&mut counter);
        counter.len()
    }

    /// Serialize to a pre-allocated `Vec<u8>`, clearing it first (amortizes allocation).
    #[inline]
    fn json_write_reuse<'a>(&self, buf: &'a mut Vec<u8>) -> &'a [u8] {
        buf.clear();
        self.json_write(buf);
        buf.as_slice()
    }

    /// Serialize to any `io::Write`.
    fn json_write_io(&self, w: impl std::io::Write) -> std::io::Result<()>
    where
        Self: Sized,
    {
        let mut w = w;
        let mut sink = IoSink::new(&mut w);
        self.json_write_sink(&mut sink);
        sink.finish()
    }
}

impl<T: ToJson + ?Sized> ToJsonExt for T {}

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
