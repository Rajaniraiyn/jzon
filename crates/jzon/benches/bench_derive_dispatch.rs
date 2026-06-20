// Focused derive-dispatch benchmarks.
// Run with: cargo bench --bench bench_derive_dispatch --features "simd,fast-float" -- derive_dispatch

#![allow(dead_code)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use jzon::FromJson;
use std::time::Duration;

#[derive(jzon::FromJson)]
struct DispatchRecord {
    id: u64,
    name: String,
    score: f64,
    active: bool,
    count: u64,
    rank: i64,
}

#[derive(jzon::FromJson)]
#[serde(deny_unknown_fields)]
struct StrictRecord {
    id: u64,
    name: String,
    score: f64,
    active: bool,
}

#[derive(jzon::FromJson)]
struct AliasRecord {
    id: u64,
    #[serde(alias = "full_name", alias = "display_name")]
    name: String,
    #[serde(alias = "points")]
    score: f64,
}

#[derive(jzon::FromJson)]
struct PhfRecord {
    f01: u64,
    f02: u64,
    f03: u64,
    f04: u64,
    f05: u64,
    f06: u64,
    f07: u64,
    f08: u64,
    f09: u64,
    f10: u64,
}

#[derive(jzon::FromJson)]
struct LongKeyRecord {
    #[serde(rename = "customer_profile_identifier")]
    customer_profile_identifier: u64,
    #[serde(rename = "subscription_billing_cycle_count")]
    subscription_billing_cycle_count: u64,
    #[serde(rename = "feature_entitlement_rollout_bucket")]
    feature_entitlement_rollout_bucket: u64,
    #[serde(rename = "last_successful_invoice_timestamp")]
    last_successful_invoice_timestamp: u64,
    #[serde(rename = "regional_compliance_policy_revision")]
    regional_compliance_policy_revision: u64,
    #[serde(rename = "experiment_assignment_checksum")]
    experiment_assignment_checksum: u64,
    #[serde(rename = "account_lifecycle_state_version")]
    account_lifecycle_state_version: u64,
}

#[derive(jzon::FromJson)]
#[rjson(trie_dispatch)]
struct TrieRecord {
    alpha: u64,
    beta: u64,
    gamma: u64,
    delta: u64,
    epsilon: u64,
    zeta: u64,
    eta: u64,
    theta: u64,
}

#[derive(jzon::FromJson)]
#[serde(tag = "type")]
enum TaggedBenchShape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Point,
}

const IN_ORDER: &str = r#"{"id":1,"name":"alice","score":9.5,"active":true,"count":7,"rank":-2}"#;
const SHUFFLED: &str = r#"{"rank":-2,"count":7,"active":true,"score":9.5,"name":"alice","id":1}"#;
const WITH_UNKNOWN: &str = r#"{"id":1,"unused":{"nested":[true,false,null]},"name":"alice","score":9.5,"active":true,"count":7,"rank":-2}"#;
const STRICT_OK: &str = r#"{"id":1,"name":"alice","score":9.5,"active":true}"#;
const STRICT_UNKNOWN: &str = r#"{"id":1,"unknown":99,"name":"alice","score":9.5,"active":true}"#;
const ALIAS_PRIMARY: &str = r#"{"id":1,"name":"alice","score":9.5}"#;
const ALIAS_ALT: &str = r#"{"id":1,"display_name":"alice","points":9.5}"#;
const PHF_INPUT: &str =
    r#"{"f01":1,"f02":2,"f03":3,"f04":4,"f05":5,"f06":6,"f07":7,"f08":8,"f09":9,"f10":10}"#;
const LONG_KEYS_INPUT: &str = r#"{"customer_profile_identifier":1,"subscription_billing_cycle_count":2,"feature_entitlement_rollout_bucket":3,"last_successful_invoice_timestamp":4,"regional_compliance_policy_revision":5,"experiment_assignment_checksum":6,"account_lifecycle_state_version":7}"#;
const TRIE_INPUT: &str =
    r#"{"alpha":1,"beta":2,"gamma":3,"delta":4,"epsilon":5,"zeta":6,"eta":7,"theta":8}"#;
const TAGGED_TAG_FIRST_CIRCLE: &str = r#"{"type":"Circle","radius":2.5}"#;
const TAGGED_TAG_LAST_RECT: &str = r#"{"width":4.0,"height":5.0,"type":"Rectangle"}"#;
const TAGGED_UNIT_POINT: &str = r#"{"type":"Point"}"#;

fn bench_ordering_and_unknowns(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        ("in_order_fields", IN_ORDER),
        ("shuffled_fields", SHUFFLED),
        ("unknown_fields_skip_value", WITH_UNKNOWN),
    ];

    let mut group = c.benchmark_group("derive_dispatch/field_order");
    group.sample_size(200);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    for &(name, input) in cases {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, s| {
            b.iter(|| black_box(DispatchRecord::from_json_str(black_box(s)).unwrap()))
        });
    }
    group.finish();
}

fn bench_strict_and_aliases(c: &mut Criterion) {
    let mut strict = c.benchmark_group("derive_dispatch/deny_unknown_fields");
    strict.sample_size(200);
    strict.warm_up_time(Duration::from_millis(300));
    strict.measurement_time(Duration::from_secs(1));
    strict.throughput(Throughput::Bytes(STRICT_OK.len() as u64));
    strict.bench_function("known_fields", |b| {
        b.iter(|| black_box(StrictRecord::from_json_str(black_box(STRICT_OK)).unwrap()))
    });
    strict.throughput(Throughput::Bytes(STRICT_UNKNOWN.len() as u64));
    strict.bench_function("unknown_field_error", |b| {
        b.iter(|| black_box(StrictRecord::from_json_str(black_box(STRICT_UNKNOWN)).is_err()))
    });
    strict.finish();

    let cases: &[(&str, &str)] = &[("primary_keys", ALIAS_PRIMARY), ("alias_keys", ALIAS_ALT)];
    let mut aliases = c.benchmark_group("derive_dispatch/aliases");
    aliases.sample_size(200);
    aliases.warm_up_time(Duration::from_millis(300));
    aliases.measurement_time(Duration::from_secs(1));
    for &(name, input) in cases {
        aliases.throughput(Throughput::Bytes(input.len() as u64));
        aliases.bench_with_input(BenchmarkId::from_parameter(name), input, |b, s| {
            b.iter(|| black_box(AliasRecord::from_json_str(black_box(s)).unwrap()))
        });
    }
    aliases.finish();
}

fn bench_dispatch_strategies(c: &mut Criterion) {
    let cases: &[(&str, &str, fn(&str))] = &[
        ("phf_threshold_struct", PHF_INPUT, |s| {
            black_box(PhfRecord::from_json_str(black_box(s)).unwrap());
        }),
        ("long_keys", LONG_KEYS_INPUT, |s| {
            black_box(LongKeyRecord::from_json_str(black_box(s)).unwrap());
        }),
        ("rjson_trie_dispatch", TRIE_INPUT, |s| {
            black_box(TrieRecord::from_json_str(black_box(s)).unwrap());
        }),
    ];

    let mut group = c.benchmark_group("derive_dispatch/strategies");
    group.sample_size(200);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    for &(name, input, parse) in cases {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, s| {
            b.iter(|| parse(black_box(s)))
        });
    }
    group.finish();
}

fn bench_internally_tagged_enums(c: &mut Criterion) {
    let cases: &[(&str, &str)] = &[
        ("tag_first_struct", TAGGED_TAG_FIRST_CIRCLE),
        ("tag_last_struct", TAGGED_TAG_LAST_RECT),
        ("tag_first_unit", TAGGED_UNIT_POINT),
    ];

    let mut group = c.benchmark_group("derive_dispatch/internally_tagged");
    group.sample_size(200);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    for &(name, input) in cases {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, s| {
            b.iter(|| black_box(TaggedBenchShape::from_json_str(black_box(s)).unwrap()))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_ordering_and_unknowns,
    bench_strict_and_aliases,
    bench_dispatch_strategies,
    bench_internally_tagged_enums,
);
criterion_main!(benches);
