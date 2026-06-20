// Focused scanner microbenchmarks.
// Run with: cargo bench --bench bench_scanner --features "simd,fast-float" -- scanner

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use jzon::{FromJson, Scanner};
use std::{sync::OnceLock, time::Duration};

#[allow(dead_code)]
#[derive(FromJson)]
struct KnownId {
    id: u64,
}

static WIDE_UNKNOWN_STRINGS: OnceLock<String> = OnceLock::new();

fn wide_unknown_strings_object() -> &'static str {
    WIDE_UNKNOWN_STRINGS.get_or_init(|| {
        let mut s = String::with_capacity(16 * 1024);
        s.push_str(r#"{"id":42"#);
        for i in 0usize..256 {
            s.push_str(&format!(
                r#","unknown_{i:03}":"abcdefghijklmnopqrstuvwxyz \"quoted\" slash \\ field {i:03}""#
            ));
        }
        s.push('}');
        s
    })
}

fn compact_object() -> &'static str {
    r#"{"id":42,"name":"alice","score":9.875,"active":true,"tags":["json","bench"],"meta":{"rank":7,"ok":false}}"#
}

fn pretty_object() -> &'static str {
    r#"{
  "id" : 42,
  "name" : "alice",
  "score" : 9.875,
  "active" : true,
  "tags" : [
    "json",
    "bench"
  ],
  "meta" : {
    "rank" : 7,
    "ok" : false
  }
}"#
}

fn bench_skip_whitespace(c: &mut Criterion) {
    let cases: &[(&str, &[u8])] = &[
        ("compact_fast_path", b"{"),
        ("pretty_spaces", b"        \n\t\r  {"),
    ];

    let mut group = c.benchmark_group("scanner/skip_whitespace");
    group.sample_size(200);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    for &(name, input) in cases {
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, bytes| {
            b.iter(|| {
                let mut scanner = Scanner::new(black_box(bytes));
                scanner.skip_whitespace();
                black_box(scanner.pos())
            })
        });
    }
    group.finish();
}

fn bench_read_key_colon(c: &mut Criterion) {
    let cases: &[(&str, &[u8])] = &[
        ("compact", br#""field_name":123"#),
        ("pretty", br#""field_name"  :  123"#),
    ];

    let mut group = c.benchmark_group("scanner/read_key_colon");
    group.sample_size(200);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    for &(name, input) in cases {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, bytes| {
            b.iter(|| {
                let mut scanner = Scanner::new(black_box(bytes));
                let key = scanner.read_key_colon().unwrap();
                black_box((key.len(), scanner.pos()))
            })
        });
    }
    group.finish();
}

fn bench_read_str(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        ("plain_short", r#""alice_wonderland_2026""#),
        (
            "plain_long",
            r#""abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789""#,
        ),
        ("utf8_short", "\"caf\u{00e9}\""),
        (
            "utf8_long",
            "\"René Müller 日本語テスト 🚀 abcdefghijklmnopqrstuvwxyz0123456789\"",
        ),
        ("escaped", r#""line\nquote\"slash\\tab\tunicode\u0041""#),
    ];

    let mut group = c.benchmark_group("scanner/read_str");
    group.sample_size(200);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    for &(name, input) in cases {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, s| {
            b.iter(|| {
                let mut scanner = Scanner::new_str(black_box(s));
                let value = scanner.read_str().unwrap();
                black_box((value.as_str().len(), scanner.pos()))
            })
        });
    }
    group.finish();
}

fn bench_read_str_heavy(c: &mut Criterion) {
    static STRING_HEAVY: OnceLock<String> = OnceLock::new();
    let payload = STRING_HEAVY.get_or_init(|| {
        let mut s = String::from('[');
        for i in 0..512 {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                r#""field_{i:03}_abcdefghijklmnopqrstuvwxyz0123456789""#
            ));
        }
        s.push(']');
        s
    });

    let mut group = c.benchmark_group("scanner/read_str_heavy");
    group.sample_size(100);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    group.throughput(Throughput::Bytes(payload.len() as u64));
    group.bench_function("parse_512_plain_strings", |b| {
        b.iter(|| {
            let mut scanner = Scanner::new_str(black_box(payload));
            scanner.expect_byte(b'[').unwrap();
            for _ in 0..512 {
                scanner.skip_whitespace();
                if scanner.peek_byte().unwrap() == b']' {
                    break;
                }
                let value = scanner.read_str().unwrap();
                black_box(value.as_str().len());
                scanner.skip_whitespace();
                scanner.expect_byte(b',').ok();
            }
            scanner.skip_whitespace();
            scanner.expect_byte(b']').unwrap();
            black_box(scanner.pos())
        })
    });
    group.finish();
}

fn bench_read_number_bytes(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        ("u64", "1844674407370955161,"),
        ("negative_float", "-1234567890.125,"),
        ("exponent", "6.02214076e23,"),
    ];

    let mut group = c.benchmark_group("scanner/read_number_bytes");
    group.sample_size(200);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    for &(name, input) in cases {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, s| {
            b.iter(|| {
                let mut scanner = Scanner::new_str(black_box(s));
                let bytes = scanner.read_number_bytes().unwrap();
                black_box((bytes.len(), scanner.pos()))
            })
        });
    }
    group.finish();
}

fn bench_skip_value(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        ("compact_object", compact_object()),
        ("pretty_object", pretty_object()),
        ("wide_unknown_strings_object", wide_unknown_strings_object()),
        (
            "escaped_string",
            r#""line\nquote\"slash\\tab\tunicode\u0041""#,
        ),
        ("numeric_array", "[1,-2,3.5,6.022e23,1844674407370955161]"),
    ];

    let mut group = c.benchmark_group("scanner/skip_value");
    group.sample_size(100);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    for &(name, input) in cases {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, s| {
            b.iter(|| {
                let mut scanner = Scanner::new_str(black_box(s));
                scanner.skip_value().unwrap();
                black_box(scanner.pos())
            })
        });
    }
    group.finish();
}

fn bench_unknown_string_fields(c: &mut Criterion) {
    let input = wide_unknown_strings_object();
    let mut group = c.benchmark_group("scanner/skip_unknown_string_fields");
    group.sample_size(100);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("derive_wide_object", |b| {
        b.iter(|| {
            let value = KnownId::from_json_str(black_box(input)).unwrap();
            black_box(value.id)
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_skip_whitespace,
    bench_read_key_colon,
    bench_read_str,
    bench_read_str_heavy,
    bench_read_number_bytes,
    bench_skip_value,
    bench_unknown_string_fields,
);
criterion_main!(benches);
