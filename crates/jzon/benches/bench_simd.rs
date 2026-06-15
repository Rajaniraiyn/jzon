// Microbench: find_quote_or_backslash kernel comparison.
// Run: cargo bench --bench bench_simd --features "simd,simd-intrinsics"
//      cargo +nightly bench --bench bench_simd --features "simd,simd-intrinsics,unstable"

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

// === Payload builders ===

/// `len` bytes of 'a' with no match.
fn no_match(len: usize) -> Vec<u8> {
    vec![b'a'; len]
}

/// `len` bytes of 'a' with a single quote at position `len - 1`.
/// Worst case for short-circuit kernels.
fn quote_at_end(len: usize) -> Vec<u8> {
    let mut v = vec![b'a'; len];
    v[len - 1] = b'"';
    v
}

/// `len` bytes of 'a' with a quote at `len / 2`.
fn quote_at_mid(len: usize) -> Vec<u8> {
    let mut v = vec![b'a'; len];
    v[len / 2] = b'"';
    v
}

// === Per-kernel benchmarks ===

fn bench_kernels(c: &mut Criterion) {
    // Sizes chosen to cover: under one 16B chunk, under 64B, common JSON string
    // (~256B), large blob (~4KB), and Twitter-test-scale (4MB).
    let sizes: &[usize] = &[64, 256, 4096, 65_536, 4 * 1024 * 1024];

    for &n in sizes {
        let mut g = c.benchmark_group(format!("find_q_or_bs/{n}B"));
        g.throughput(Throughput::Bytes(n as u64));

        // Three workloads per size: no-match (full scan), mid, end.
        for (label, builder) in [
            ("nomatch", no_match as fn(usize) -> Vec<u8>),
            ("midmatch", quote_at_mid),
            ("endmatch", quote_at_end),
        ] {
            let input = builder(n);

            // --- scalar u64 SWAR (baseline) ---
            g.bench_with_input(BenchmarkId::new(format!("swar_u64/{label}"), n), &input, |b, inp| {
                b.iter(|| black_box(jzon::simd::find_quote_or_backslash(black_box(inp), 0)))
            });

            // --- u128 SWAR ---
            #[cfg(feature = "simd")]
            g.bench_with_input(BenchmarkId::new(format!("swar_u128/{label}"), n), &input, |b, inp| {
                b.iter(|| black_box(jzon::simd::find_quote_or_backslash_simd16(black_box(inp), 0)))
            });

            // --- portable_simd 32B ---
            #[cfg(all(feature = "simd", feature = "unstable"))]
            g.bench_with_input(BenchmarkId::new(format!("portable_32/{label}"), n), &input, |b, inp| {
                b.iter(|| black_box(jzon::simd::find_quote_or_backslash_portable32(black_box(inp), 0)))
            });

            // --- portable_simd 64B ---
            #[cfg(all(feature = "simd", feature = "unstable"))]
            g.bench_with_input(BenchmarkId::new(format!("portable_64/{label}"), n), &input, |b, inp| {
                b.iter(|| black_box(jzon::simd::find_quote_or_backslash_portable64(black_box(inp), 0)))
            });

            // --- std::arch NEON 16B ---
            #[cfg(all(feature = "simd-intrinsics", target_arch = "aarch64"))]
            g.bench_with_input(BenchmarkId::new(format!("neon_16/{label}"), n), &input, |b, inp| {
                b.iter(|| {
                    black_box(jzon::simd_arch::neon::find_quote_or_backslash_16(
                        black_box(inp),
                        0,
                    ))
                })
            });

            // --- std::arch NEON 64B ---
            #[cfg(all(feature = "simd-intrinsics", target_arch = "aarch64"))]
            g.bench_with_input(BenchmarkId::new(format!("neon_64/{label}"), n), &input, |b, inp| {
                b.iter(|| {
                    black_box(jzon::simd_arch::neon::find_quote_or_backslash_64(
                        black_box(inp),
                        0,
                    ))
                })
            });

            // --- std::arch SSE2 16B (x86_64 baseline) ---
            #[cfg(all(feature = "simd-intrinsics", target_arch = "x86_64"))]
            g.bench_with_input(BenchmarkId::new(format!("sse2_16/{label}"), n), &input, |b, inp| {
                b.iter(|| {
                    black_box(jzon::simd_arch::x86::find_quote_or_backslash_16(
                        black_box(inp),
                        0,
                    ))
                })
            });

            // --- std::arch AVX2 32B (x86_64, runtime-detected) ---
            #[cfg(all(feature = "simd-intrinsics", target_arch = "x86_64"))]
            g.bench_with_input(BenchmarkId::new(format!("avx2_32/{label}"), n), &input, |b, inp| {
                b.iter(|| {
                    black_box(jzon::simd_arch::x86::find_quote_or_backslash_32(
                        black_box(inp),
                        0,
                    ))
                })
            });

        }
        g.finish();
    }

    // find_escape: nomatch only — we already know match-position behaviour
    // mirrors find_q_or_bs.
    let escape_sizes: &[usize] = &[256, 4096, 65_536, 4 * 1024 * 1024];
    for &n in escape_sizes {
        let mut g = c.benchmark_group(format!("find_escape/{n}B"));
        g.throughput(Throughput::Bytes(n as u64));
        let input = no_match(n);

        #[cfg(all(feature = "simd-intrinsics", target_arch = "aarch64"))]
        {
            g.bench_with_input(BenchmarkId::new("neon_16/nomatch", n), &input, |b, inp| {
                b.iter(|| black_box(jzon::simd_arch::neon::find_escape_16(black_box(inp), 0)))
            });
            g.bench_with_input(BenchmarkId::new("neon_64/nomatch", n), &input, |b, inp| {
                b.iter(|| black_box(jzon::simd_arch::neon::find_escape_64(black_box(inp), 0)))
            });
        }
        #[cfg(all(feature = "simd-intrinsics", target_arch = "x86_64"))]
        {
            g.bench_with_input(BenchmarkId::new("sse2_16/nomatch", n), &input, |b, inp| {
                b.iter(|| black_box(jzon::simd_arch::x86::find_escape_16(black_box(inp), 0)))
            });
            g.bench_with_input(BenchmarkId::new("avx2_32/nomatch", n), &input, |b, inp| {
                b.iter(|| black_box(jzon::simd_arch::x86::find_escape_32(black_box(inp), 0)))
            });
        }
        g.finish();
    }
}

criterion_group!(benches, bench_kernels);
criterion_main!(benches);
