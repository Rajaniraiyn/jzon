//! `ToJson` trait and primitive implementations.

mod sink;

pub use sink::{IoSink, JsonSink, LengthCounter, VecSink};

use std::collections::{BTreeMap, HashMap};

pub trait ToJson {
    fn json_write(&self, w: &mut Vec<u8>);

    /// Serialize directly into any supported [`JsonSink`].
    ///
    /// Types that override this method avoid staging through a temporary [`Vec`].
    /// The default implementation calls [`json_write`](ToJson::json_write) and
    /// copies the result into `w`.
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S)
    where
        Self: Sized,
    {
        let mut buf = Vec::with_capacity(self.json_size_hint());
        self.json_write(&mut buf);
        w.extend(&buf);
    }

    /// Hint for the approximate number of bytes this value will serialize to.
    ///
    /// This is used by `to_json_bytes` to pre-allocate the output buffer,
    /// avoiding reallocations for the common case.  Implementations should
    /// return a value that is *at least* as large as the serialized form in
    /// the common case — over-estimating is fine, under-estimating causes a
    /// single reallocation.  The default (64) is conservative.
    #[inline]
    fn json_size_hint(&self) -> usize { 64 }

    #[must_use]
    fn to_json_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.json_size_hint());
        self.json_write(&mut buf);
        buf
    }

    #[must_use]
    fn to_json_string(&self) -> String {
        // SAFETY: ToJson implementations only write valid UTF-8 bytes.
        // The expect path is unreachable for any correct impl; using expect (not
        // unwrap_unchecked) to preserve a clear panic message for buggy custom impls.
        String::from_utf8(self.to_json_bytes())
            .expect("ToJson implementations always emit valid UTF-8")
    }
}

// ── string escaping ───────────────────────────────────────────────────────────
//
// `write_escaped_str` delegates byte scanning to `crate::simd::find_escape`,
// which dispatches to the widest available implementation:
//   - nightly + simd feature  → 32-byte std::simd lanes  (find_escape_simd32)
//   - stable + simd feature   → 16-byte u128 SWAR        (find_escape_simd16)
//   - no simd feature         → scalar byte-by-byte      (find_escape_scalar)
//
// This keeps ser.rs free of SWAR arithmetic — all the bit tricks live in simd.rs.

#[inline]
pub fn write_escaped_str(s: &str, w: &mut Vec<u8>) {
    write_escaped_str_sink(s, &mut VecSink(w));
}

#[inline]
pub fn write_escaped_str_sink<S: JsonSink>(s: &str, w: &mut S) {
    w.push(b'"');
    // Pre-reserve: common case is no escaping, so reserve s.len() + 1 (closing quote).
    // Avoids all reallocations in the fast (no-escape) path.
    w.reserve(s.len() + 1);
    let bytes = s.as_bytes();
    let mut start = 0usize; // start of current unescaped run

    let mut i = start;
    while i < bytes.len() {
        // Find the next byte that needs escaping using the widest available path.
        let stop = crate::simd::find_escape(bytes, i);
        if stop >= bytes.len() {
            // No more bytes need escaping; flush the rest in one go.
            break;
        }
        // Flush safe bytes [start..stop], then emit the escape sequence.
        w.extend(&bytes[start..stop]);
        escape_one(bytes[stop], w);
        i = stop + 1;
        start = i;
    }

    // Flush the final safe run.
    w.extend(&bytes[start..]);
    w.push(b'"');
}

#[inline(always)]
fn escape_one<S: JsonSink>(b: u8, w: &mut S) {
    match b {
        b'"'  => w.extend(b"\\\""),
        b'\\' => w.extend(b"\\\\"),
        b'\n' => w.extend(b"\\n"),
        b'\r' => w.extend(b"\\r"),
        b'\t' => w.extend(b"\\t"),
        0x08  => w.extend(b"\\b"),
        0x0C  => w.extend(b"\\f"),
        b     => {
            // Other control characters as \u00XX
            let hi = b >> 4;
            let lo = b & 0xF;
            w.extend(&[
                b'\\', b'u', b'0', b'0',
                if hi < 10 { b'0' + hi } else { b'a' + hi - 10 },
                if lo < 10 { b'0' + lo } else { b'a' + lo - 10 },
            ]);
        }
    }
}

// ── primitive impls ───────────────────────────────────────────────────────────

impl ToJson for bool {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_bool(*self, &mut VecSink(w));
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_bool(*self, w);
    }
    #[inline] fn json_size_hint(&self) -> usize { 5 } // "false"
}

#[inline]
fn write_bool<S: JsonSink>(v: bool, w: &mut S) {
    w.extend(if v { b"true" } else { b"false" });
}

impl ToJson for str {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_escaped_str(self, w);
    }
    #[inline] fn json_size_hint(&self) -> usize { self.len() + 2 }
}

impl ToJson for String {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_escaped_str(self, w);
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_escaped_str_sink(self, w);
    }
    #[inline] fn json_size_hint(&self) -> usize { self.len() + 2 }
}

impl ToJson for crate::scanner::JsonStr<'_> {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_json_str(self, &mut VecSink(w));
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_json_str(self, w);
    }
    #[inline]
    fn json_size_hint(&self) -> usize { self.as_str().len() + 2 }
}

#[inline]
fn write_json_str<S: JsonSink>(value: &crate::scanner::JsonStr<'_>, w: &mut S) {
    use crate::scanner::JsonStr;
    match value {
        JsonStr::BorrowedNoEsc(s) => {
            // Provably escape-free — skip find_escape scan.
            w.reserve(s.len() + 2);
            w.push(b'"');
            w.extend(s.as_bytes());
            w.push(b'"');
        }
        JsonStr::Borrowed(s) => write_escaped_str_sink(s, w),
        JsonStr::Owned(s) => write_escaped_str_sink(s, w),
    }
}

impl<T: ToJson> ToJson for &T {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        (**self).json_write(w);
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        (**self).json_write_sink(w);
    }
    #[inline] fn json_size_hint(&self) -> usize { (**self).json_size_hint() }
}

impl ToJson for &str {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_escaped_str(self, w);
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_escaped_str_sink(self, w);
    }
    #[inline] fn json_size_hint(&self) -> usize { self.len() + 2 }
}

impl<T: ToJson> ToJson for Box<T> {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        (**self).json_write(w);
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        (**self).json_write_sink(w);
    }
    #[inline] fn json_size_hint(&self) -> usize { (**self).json_size_hint() }
}

impl<T: ToJson> ToJson for Option<T> {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_option(self, &mut VecSink(w));
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_option(self, w);
    }
    #[inline]
    fn json_size_hint(&self) -> usize {
        match self {
            Some(v) => v.json_size_hint(),
            None    => 4, // "null"
        }
    }
}

#[inline]
fn write_option<T: ToJson, S: JsonSink>(value: &Option<T>, w: &mut S) {
    match value {
        Some(v) => v.json_write_sink(w),
        None    => w.extend(b"null"),
    }
}

impl<T: ToJson> ToJson for Vec<T> {
    fn json_write(&self, w: &mut Vec<u8>) {
        write_seq(self.iter(), &mut VecSink(w));
    }
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_seq(self.iter(), w);
    }
    #[inline]
    fn json_size_hint(&self) -> usize {
        if self.is_empty() { return 2; }
        // Use the first element's hint as a sample; add separating commas.
        2 + self.len() * (self[0].json_size_hint() + 1)
    }
}

impl<T: ToJson, const N: usize> ToJson for [T; N] {
    fn json_write(&self, w: &mut Vec<u8>) {
        write_seq(self.iter(), &mut VecSink(w));
    }
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_seq(self.iter(), w);
    }
    #[inline]
    fn json_size_hint(&self) -> usize {
        if N == 0 { return 2; }
        2 + N * (self[0].json_size_hint() + 1)
    }
}

impl<T: ToJson> ToJson for [T] {
    fn json_write(&self, w: &mut Vec<u8>) {
        write_seq(self.iter(), &mut VecSink(w));
    }
    #[inline]
    fn json_size_hint(&self) -> usize {
        if self.is_empty() { return 2; }
        2 + self.len() * (self[0].json_size_hint() + 1)
    }
}

impl<T: ToJson> ToJson for &[T] {
    fn json_write(&self, w: &mut Vec<u8>) {
        write_seq(self.iter(), &mut VecSink(w));
    }
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_seq(self.iter(), w);
    }
    #[inline]
    fn json_size_hint(&self) -> usize {
        if self.is_empty() { return 2; }
        2 + self.len() * (self[0].json_size_hint() + 1)
    }
}

#[inline]
fn write_seq<'a, T: ToJson + 'a, I: Iterator<Item = &'a T>, S: JsonSink>(items: I, w: &mut S) {
    w.push(b'[');
    let mut first = true;
    for item in items {
        if !first {
            w.push(b',');
        }
        item.json_write_sink(w);
        first = false;
    }
    w.push(b']');
}

// Note: A specialized impl for Vec<f64> is not possible on stable Rust due to
// the coherence rules (conflicts with impl<T: ToJson> ToJson for Vec<T>).
// The generic Vec<T> impl with json_size_hint delegating to f64::json_size_hint (10)
// covers the f64 case correctly through monomorphization.

// ── integer writers (no format! overhead) ─────────────────────────────────────

#[inline]
pub fn write_u64(n: u64, w: &mut Vec<u8>) {
    write_u64_sink(n, &mut VecSink(w));
}

#[inline(always)]
pub fn write_u64_sink<S: JsonSink>(mut n: u64, w: &mut S) {
    if n == 0 { w.push(b'0'); return; }
    let mut tmp = [0u8; 20];
    let mut len = 0usize;
    while n > 0 { tmp[len] = b'0' + (n % 10) as u8; n /= 10; len += 1; }
    tmp[..len].reverse();
    w.extend(&tmp[..len]);
}

#[inline]
pub fn write_i64(n: i64, w: &mut Vec<u8>) {
    write_i64_sink(n, &mut VecSink(w));
}

#[inline(always)]
pub fn write_i64_sink<S: JsonSink>(n: i64, w: &mut S) {
    if n < 0 { w.push(b'-'); write_u64_sink(n.unsigned_abs(), w); } else { write_u64_sink(n as u64, w); }
}

macro_rules! impl_uint {
    ($($t:ty, $hint:expr),*) => {$(
        impl ToJson for $t {
            #[inline] fn json_write(&self, w: &mut Vec<u8>) { write_u64_sink(*self as u64, &mut VecSink(w)); }
            #[inline] fn json_write_sink<S: JsonSink>(&self, w: &mut S) { write_u64_sink(*self as u64, w); }
            #[inline] fn json_size_hint(&self) -> usize { $hint }
        }
    )*};
}
macro_rules! impl_sint {
    ($($t:ty, $hint:expr),*) => {$(
        impl ToJson for $t {
            #[inline] fn json_write(&self, w: &mut Vec<u8>) { write_i64_sink(*self as i64, &mut VecSink(w)); }
            #[inline] fn json_write_sink<S: JsonSink>(&self, w: &mut S) { write_i64_sink(*self as i64, w); }
            #[inline] fn json_size_hint(&self) -> usize { $hint }
        }
    )*};
}
// Tight upper bounds (max digit count including sign for signed types):
//   u8:3, u16:5, u32:10, u64:20, u128:39, usize:20
//   i8:4, i16:6, i32:11, i64:20, i128:40, isize:20
impl_uint!(u8, 3, u16, 5, u32, 10, u64, 20, usize, 20);
impl_sint!(i8, 4, i16, 6, i32, 11, i64, 20, isize, 20);

// u128 / i128: cannot pass through u64/i64, need dedicated digit writers.
#[inline]
fn write_u128_sink<S: JsonSink>(mut n: u128, w: &mut S) {
    if n == 0 { w.push(b'0'); return; }
    let mut tmp = [0u8; 39];
    let mut len = 0usize;
    while n > 0 { tmp[len] = b'0' + (n % 10) as u8; n /= 10; len += 1; }
    tmp[..len].reverse();
    w.extend(&tmp[..len]);
}
impl ToJson for u128 {
    #[inline] fn json_write(&self, w: &mut Vec<u8>) { write_u128_sink(*self, &mut VecSink(w)); }
    #[inline] fn json_write_sink<S: JsonSink>(&self, w: &mut S) { write_u128_sink(*self, w); }
    #[inline] fn json_size_hint(&self) -> usize { 39 }
}
impl ToJson for i128 {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_i128_sink(*self, &mut VecSink(w));
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_i128_sink(*self, w);
    }
    #[inline] fn json_size_hint(&self) -> usize { 40 }
}

#[inline]
fn write_i128_sink<S: JsonSink>(n: i128, w: &mut S) {
    if n < 0 { w.push(b'-'); write_u128_sink(n.unsigned_abs(), w); } else { write_u128_sink(n as u128, w); }
}

impl ToJson for f64 {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_f64(*self, &mut VecSink(w));
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_f64(*self, w);
    }
    /// ryu's worst-case output for f64 is 24 characters, but the practical output for
    /// typical floats (integers, short decimals, small exponents) is 2–6 characters.
    /// Using 10 as the hint covers the vast majority of real-world floats without the
    /// 24-byte worst-case causing 96-byte allocations for small structs.  Under-estimation
    /// only causes a single reallocation, whereas over-estimation wastes allocator headroom
    /// and pushes small structs into larger (slower) allocator size classes.
    #[inline] fn json_size_hint(&self) -> usize { 10 }
}

impl ToJson for f32 {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_f32(*self, &mut VecSink(w));
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_f32(*self, w);
    }
    /// ryu's output for f32 is at most 14 characters.
    #[inline] fn json_size_hint(&self) -> usize { 14 }
}

#[inline]
fn write_f64<S: JsonSink>(n: f64, w: &mut S) {
    if !n.is_finite() { w.extend(b"null"); return; }
    // ECMA-404 / ECMA-262 §24.5.2.4: -0 must serialise as "0".
    // IEEE 754: -0.0 == 0.0, so this check catches both.
    if n == 0.0 { w.extend(b"0"); return; }
    #[cfg(feature = "zmij-float-ser")]
    {
        let mut buf = zmij::Buffer::new();
        w.extend(buf.format_finite(n).as_bytes());
        return;
    }
    #[cfg(all(feature = "fast-float", not(feature = "zmij-float-ser")))]
    {
        let mut buf = ryu::Buffer::new();
        w.extend(buf.format_finite(n).as_bytes());
        return;
    }
    #[cfg(not(any(feature = "fast-float", feature = "zmij-float-ser")))]
    w.extend(format!("{}", n).as_bytes());
}

#[inline]
fn write_f32<S: JsonSink>(n: f32, w: &mut S) {
    if !n.is_finite() { w.extend(b"null"); return; }
    #[cfg(feature = "zmij-float-ser")]
    {
        let mut buf = zmij::Buffer::new();
        w.extend(buf.format_finite(n).as_bytes());
        return;
    }
    #[cfg(all(feature = "fast-float", not(feature = "zmij-float-ser")))]
    {
        let mut buf = ryu::Buffer::new();
        w.extend(buf.format_finite(n).as_bytes());
        return;
    }
    #[cfg(not(any(feature = "fast-float", feature = "zmij-float-ser")))]
    w.extend(format!("{}", n).as_bytes());
}

// ── char ──────────────────────────────────────────────────────────────────────

impl ToJson for char {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        write_char(*self, &mut VecSink(w));
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_char(*self, w);
    }
    /// At most 4 UTF-8 bytes + 2 surrounding quotes.
    #[inline] fn json_size_hint(&self) -> usize { 6 }
}

#[inline]
fn write_char<S: JsonSink>(c: char, w: &mut S) {
    let mut buf = [0u8; 4];
    write_escaped_str_sink(c.encode_utf8(&mut buf), w);
}

// ── unit → null ───────────────────────────────────────────────────────────────

impl ToJson for () {
    #[inline]
    fn json_write(&self, w: &mut Vec<u8>) {
        w.extend_from_slice(b"null");
    }
    #[inline]
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        w.extend(b"null");
    }
    #[inline] fn json_size_hint(&self) -> usize { 4 }
}

// ── HashMap / BTreeMap → JSON objects ────────────────────────────────────────

impl<K: ToJson, V: ToJson> ToJson for HashMap<K, V> {
    fn json_write(&self, w: &mut Vec<u8>) {
        write_map(self.iter(), &mut VecSink(w));
    }
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_map(self.iter(), w);
    }
    #[inline]
    fn json_size_hint(&self) -> usize {
        if self.is_empty() { return 2; }
        let (k, v) = self.iter().next().unwrap();
        2 + self.len() * (k.json_size_hint() + 1 + v.json_size_hint() + 1)
    }
}

impl<K: ToJson, V: ToJson> ToJson for BTreeMap<K, V> {
    fn json_write(&self, w: &mut Vec<u8>) {
        write_map(self.iter(), &mut VecSink(w));
    }
    fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
        write_map(self.iter(), w);
    }
    #[inline]
    fn json_size_hint(&self) -> usize {
        if self.is_empty() { return 2; }
        let (k, v) = self.iter().next().unwrap();
        2 + self.len() * (k.json_size_hint() + 1 + v.json_size_hint() + 1)
    }
}

#[inline]
fn write_map<'a, K: ToJson + 'a, V: ToJson + 'a, I: Iterator<Item = (&'a K, &'a V)>, S: JsonSink>(
    entries: I,
    w: &mut S,
) {
    w.push(b'{');
    let mut first = true;
    for (k, v) in entries {
        if !first {
            w.push(b',');
        }
        first = false;
        k.json_write_sink(w);
        w.push(b':');
        v.json_write_sink(w);
    }
    w.push(b'}');
}

// ── tuples → JSON arrays (1- to 12-element) ───────────────────────────────────

macro_rules! impl_tuple_to_json {
    ($($T:ident . $idx:tt),+) => {
        impl<$($T: ToJson),+> ToJson for ($($T,)+) {
            fn json_write(&self, w: &mut Vec<u8>) {
                tuple_write_sink!(self, &mut VecSink(w) $(, $idx)+);
            }
            fn json_write_sink<S: JsonSink>(&self, w: &mut S) {
                tuple_write_sink!(self, w $(, $idx)+);
            }
            #[inline]
            fn json_size_hint(&self) -> usize {
                2 + $( self.$idx.json_size_hint() + 1 + )+ 0
                  - 1 // subtract trailing extra comma count
            }
        }
    };
}

macro_rules! tuple_write_sink {
    ($self:ident, $w:expr $(, $idx:tt)+) => {{
        let w = $w;
        w.push(b'[');
        let mut first = true;
        $( if !first { w.push(b','); } first = false; $self.$idx.json_write_sink(w); )+
        let _ = first;
        w.push(b']');
    }};
}

impl_tuple_to_json!(A.0);
impl_tuple_to_json!(A.0, B.1);
impl_tuple_to_json!(A.0, B.1, C.2);
impl_tuple_to_json!(A.0, B.1, C.2, D.3);
impl_tuple_to_json!(A.0, B.1, C.2, D.3, E.4);
impl_tuple_to_json!(A.0, B.1, C.2, D.3, E.4, F.5);
impl_tuple_to_json!(A.0, B.1, C.2, D.3, E.4, F.5, G.6);
impl_tuple_to_json!(A.0, B.1, C.2, D.3, E.4, F.5, G.6, H.7);
impl_tuple_to_json!(A.0, B.1, C.2, D.3, E.4, F.5, G.6, H.7, I.8);
impl_tuple_to_json!(A.0, B.1, C.2, D.3, E.4, F.5, G.6, H.7, I.8, J.9);
impl_tuple_to_json!(A.0, B.1, C.2, D.3, E.4, F.5, G.6, H.7, I.8, J.9, K.10);
impl_tuple_to_json!(A.0, B.1, C.2, D.3, E.4, F.5, G.6, H.7, I.8, J.9, K.10, L.11);
