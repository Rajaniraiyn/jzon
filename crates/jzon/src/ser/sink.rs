//! Sealed JSON output sinks for zero-allocation serialization paths.

use std::io;

mod sealed {
    pub trait Sealed {}
}

/// Output target for [`crate::ToJson::json_write_sink`].
///
/// External crates cannot implement this trait; only built-in sinks are supported
/// in this phase.
pub trait JsonSink: sealed::Sealed {
    /// Append one byte. After overflow/error, subsequent calls are no-ops.
    fn push(&mut self, b: u8);

    /// Append a byte slice. After overflow/error, subsequent calls are no-ops.
    fn extend(&mut self, bs: &[u8]);

    /// Whether all writes succeeded (no overflow / I/O error).
    #[inline]
    fn is_ok(&self) -> bool {
        true
    }

    /// Hint for upcoming writes; default is a no-op.
    #[inline]
    fn reserve(&mut self, _additional: usize) {}
}

/// Adapter so [`JsonSink`] methods can target a [`Vec<u8>`].
pub struct VecSink<'a>(pub &'a mut Vec<u8>);

impl sealed::Sealed for VecSink<'_> {}

impl<'a> JsonSink for VecSink<'a> {
    #[inline]
    fn push(&mut self, b: u8) {
        self.0.push(b);
    }

    #[inline]
    fn extend(&mut self, bs: &[u8]) {
        self.0.extend_from_slice(bs);
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional);
    }
}

/// Count serialized bytes without allocating output storage.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LengthCounter {
    len: usize,
}

impl LengthCounter {
    #[inline]
    pub const fn new() -> Self {
        Self { len: 0 }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl sealed::Sealed for LengthCounter {}

impl JsonSink for LengthCounter {
    #[inline]
    fn push(&mut self, _b: u8) {
        self.len += 1;
    }

    #[inline]
    fn extend(&mut self, bs: &[u8]) {
        self.len += bs.len();
    }
}

/// Adapter that writes JSON bytes to any [`io::Write`] target.
pub struct IoSink<'a, W: io::Write + ?Sized> {
    w: &'a mut W,
    ok: bool,
    err: Option<io::Error>,
}

impl<'a, W: io::Write + ?Sized> IoSink<'a, W> {
    #[inline]
    pub fn new(w: &'a mut W) -> Self {
        Self { w, ok: true, err: None }
    }

    /// Complete the write and return the first I/O error, if any.
    #[inline]
    pub fn finish(self) -> io::Result<()> {
        match self.err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

impl<W: io::Write + ?Sized> sealed::Sealed for IoSink<'_, W> {}

impl<W: io::Write + ?Sized> IoSink<'_, W> {
    #[inline]
    fn record_err(&mut self, err: io::Error) {
        if self.ok {
            self.ok = false;
            self.err = Some(err);
        }
    }
}

impl<W: io::Write + ?Sized> JsonSink for IoSink<'_, W> {
    #[inline]
    fn push(&mut self, b: u8) {
        if !self.ok {
            return;
        }
        if let Err(e) = self.w.write_all(&[b]) {
            self.record_err(e);
        }
    }

    #[inline]
    fn extend(&mut self, bs: &[u8]) {
        if !self.ok {
            return;
        }
        if let Err(e) = self.w.write_all(bs) {
            self.record_err(e);
        }
    }

    #[inline]
    fn is_ok(&self) -> bool {
        self.ok
    }
}

impl<const N: usize> sealed::Sealed for crate::fixed::FixedBuf<N> {}

impl<const N: usize> JsonSink for crate::fixed::FixedBuf<N> {
    #[inline]
    fn push(&mut self, b: u8) {
        self.sink_push(b);
    }

    #[inline]
    fn extend(&mut self, bs: &[u8]) {
        self.sink_extend(bs);
    }

    #[inline]
    fn is_ok(&self) -> bool {
        self.sink_ok()
    }
}
