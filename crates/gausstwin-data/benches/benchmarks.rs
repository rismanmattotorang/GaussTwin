#![feature(test)]
extern crate test;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use futures::StreamExt;
use gausstwin_data::{
    create_unified_store, CacheConfig, DbConfig, FilterCondition, FilterOperator, HybridRecord,
    MetricsConfig, PoolConfig, QueryFilters, ScalarData, UnifiedStore, UnifiedStoreConfig, Value,
    VectorData, VectorStoreConfig,
};
use ndarray::Array1;
use std::time::Duration;
use tokio::runtime::Runtime;

fn create_test_store() -> Box<dyn UnifiedStore> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let config = UnifiedStoreConfig {
            vector_config: VectorStoreConfig {
                url: "memory://bench".to_string(),
                dimension: 128,
                index_type: "flat".to_string(),
                metric_type: "l2".to_string(),
                nprobe: 10,
                ef_construction: 100,
                ef_search: 50,
            },
            db_config: DbConfig {
                url: "memory://bench".to_string(),
                max_connections: 10,
                min_connections: 2,
                connect_timeout: Duration::from_secs(5),
                idle_timeout: Duration::from_secs(300),
            },
            cache_config: Some(CacheConfig {
                max_size: 10000,
                ttl: Duration::from_secs(300),
                refresh_ahead: true,
            }),
            pool_config: PoolConfig {
                max_size: 10,
                min_idle: 2,
                max_lifetime: Some(Duration::from_secs(3600)),
                idle_timeout: Some(Duration::from_secs(300)),
                connection_timeout: Duration::from_secs(5),
            },
            metrics_config: MetricsConfig {
                enabled: true,
                prefix: "bench".to_string(),
                report_interval: Duration::from_secs(1),
            },
        };

        create_unified_store(config).await.unwrap()
    })
}

fn create_test_vector_data(dimension: usize) -> VectorData {
    let vector = Array1::from_vec((0..dimension).map(|i| i as f32).collect());
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("bench_key".to_string(), "bench_value".to_string());

    VectorData {
        vector,
        metadata,
        dimension,
        namespace: Some("bench_namespace".to_string()),
    }
}

fn create_test_scalar_data() -> ScalarData {
    let mut fields = std::collections::HashMap::new();
    fields.insert(
        "string_field".to_string(),
        Value::String("bench".to_string()),
    );
    fields.insert("int_field".to_string(), Value::Integer(42));
    fields.insert("float_field".to_string(), Value::Float(3.14));

    ScalarData {
        fields,
        schema_version: 1,
        collection: Some("bench_collection".to_string()),
    }
}

fn bench_store_hybrid(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let store = create_test_store();
    let vector_data = create_test_vector_data(128);
    let scalar_data = create_test_scalar_data();

    let mut group = c.benchmark_group("store_hybrid");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("single_store", |b| {
        b.iter(|| {
            rt.block_on(async {
                store
                    .store_hybrid(
                        black_box("bench_key"),
                        black_box(&vector_data),
                        black_box(&scalar_data),
                    )
                    .await
                    .unwrap()
            })
        })
    });

    group.finish();
}

fn bench_batch_store(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let store = create_test_store();

    let mut group = c.benchmark_group("batch_store");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(15));

    for size in [10, 100, 1000].iter() {
        let mut records = Vec::new();
        for i in 0..*size {
            let vector_data = create_test_vector_data(128);
            let mut scalar_data = create_test_scalar_data();
            scalar_data
                .fields
                .insert("index".to_string(), Value::Integer(i));

            records.push(HybridRecord {
                key: format!("bench_key_{}", i),
                vector_data,
                scalar_data,
            });
        }

        group.bench_with_input(
            BenchmarkId::new("batch_size", size),
            &records,
            |b, records| {
                b.iter(|| {
                    rt.block_on(async {
                        store.batch_store_hybrid(black_box(records)).await.unwrap()
                    })
                })
            },
        );
    }

    group.finish();
}

fn bench_get_hybrid(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let store = create_test_store();
    let vector_data = create_test_vector_data(128);
    let scalar_data = create_test_scalar_data();

    // Setup: Store test data
    rt.block_on(async {
        store
            .store_hybrid("bench_get", &vector_data, &scalar_data)
            .await
            .unwrap();
    });

    let mut group = c.benchmark_group("get_hybrid");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(10));

    // First get (cache miss)
    group.bench_function("cache_miss", |b| {
        b.iter(|| rt.block_on(async { store.get_hybrid(black_box("bench_get")).await.unwrap() }))
    });

    // Second get (cache hit)
    group.bench_function("cache_hit", |b| {
        b.iter(|| rt.block_on(async { store.get_hybrid(black_box("bench_get")).await.unwrap() }))
    });

    group.finish();
}

fn bench_hybrid_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let store = create_test_store();

    // Setup: Store test data
    rt.block_on(async {
        for i in 0..1000 {
            let vector_data = create_test_vector_data(128);
            let mut scalar_data = create_test_scalar_data();
            scalar_data
                .fields
                .insert("index".to_string(), Value::Integer(i));

            store
                .store_hybrid(&format!("bench_search_{}", i), &vector_data, &scalar_data)
                .await
                .unwrap();
        }
    });

    let query_vector = Array1::from_vec((0..128).map(|i| i as f32).collect());
    let scalar_filters = QueryFilters {
        conditions: vec![FilterCondition {
            field: "int_field".to_string(),
            operator: crate::ComparisonOperator::Eq,
            value: Value::Integer(42),
        }],
        combine_operator: FilterOperator::And,
    };

    let mut group = c.benchmark_group("hybrid_search");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(15));

    for limit in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("limit", limit), limit, |b, &limit| {
            b.iter(|| {
                rt.block_on(async {
                    store
                        .hybrid_search(
                            black_box(&query_vector.as_slice().unwrap()),
                            black_box(&scalar_filters),
                            black_box(limit),
                        )
                        .await
                        .unwrap()
                })
            })
        });
    }

    group.finish();
}

fn bench_streaming_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let store = create_test_store();

    // Setup: Store test data
    rt.block_on(async {
        for i in 0..1000 {
            let vector_data = create_test_vector_data(128);
            let mut scalar_data = create_test_scalar_data();
            scalar_data
                .fields
                .insert("index".to_string(), Value::Integer(i));

            store
                .store_hybrid(&format!("bench_stream_{}", i), &vector_data, &scalar_data)
                .await
                .unwrap();
        }
    });

    let query_vector = Array1::from_vec((0..128).map(|i| i as f32).collect());
    let scalar_filters = QueryFilters {
        conditions: vec![FilterCondition {
            field: "int_field".to_string(),
            operator: crate::ComparisonOperator::Eq,
            value: Value::Integer(42),
        }],
        combine_operator: FilterOperator::And,
    };

    let mut group = c.benchmark_group("streaming_search");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(20));

    for batch_size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("batch_size", batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut stream = store
                            .stream_hybrid_search(
                                black_box(&query_vector.as_slice().unwrap()),
                                black_box(&scalar_filters),
                                black_box(batch_size),
                            )
                            .await
                            .unwrap();

                        let mut results = Vec::new();
                        while let Some(result) = stream.next().await {
                            results.push(result.unwrap());
                        }
                        results
                    })
                })
            },
        );
    }

    group.finish();
}

fn bench_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let store = create_test_store();
    let store = std::sync::Arc::new(store);

    let mut group = c.benchmark_group("concurrent_operations");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(20));

    for num_concurrent in [5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::new("num_concurrent", num_concurrent),
            num_concurrent,
            |b, &num_concurrent| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = Vec::new();

                        for i in 0..num_concurrent {
                            let store_clone = store.clone();
                            let handle = tokio::spawn(async move {
                                let vector_data = create_test_vector_data(128);
                                let mut scalar_data = create_test_scalar_data();
                                scalar_data
                                    .fields
                                    .insert("thread_id".to_string(), Value::Integer(i));

                                let key = format!("bench_concurrent_{}", i);
                                let id = store_clone
                                    .store_hybrid(&key, &vector_data, &scalar_data)
                                    .await
                                    .unwrap();

                                // Mix operations
                                if i % 2 == 0 {
                                    store_clone.get_hybrid(&key).await.unwrap();
                                } else {
                                    store_clone
                                        .hybrid_search(
                                            &vector_data.vector.as_slice().unwrap(),
                                            &QueryFilters {
                                                conditions: vec![],
                                                combine_operator: FilterOperator::And,
                                            },
                                            10,
                                        )
                                        .await
                                        .unwrap();
                                }
                            });
                            handles.push(handle);
                        }

                        for handle in handles {
                            handle.await.unwrap();
                        }
                    })
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_store_hybrid,
    bench_batch_store,
    bench_get_hybrid,
    bench_hybrid_search,
    bench_streaming_search,
    bench_concurrent_operations,
);
criterion_main!(benches);
