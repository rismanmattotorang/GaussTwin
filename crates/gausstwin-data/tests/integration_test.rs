//! Integration tests for the unified data store, exercised against the in-memory
//! backend created by `create_unified_store`.

use futures::StreamExt;
use gausstwin_data::{
    create_unified_store, CacheConfig, DbConfig, HybridData, HybridRecord, MetricsConfig,
    PoolConfig, QueryFilters, ScalarData, UnifiedStore, UnifiedStoreConfig, VectorData,
    VectorStoreConfig,
};
use std::time::Duration;

const DIM: usize = 8;

fn test_config() -> UnifiedStoreConfig {
    UnifiedStoreConfig {
        vector_config: VectorStoreConfig {
            dimension: DIM,
            distance_type: "l2".to_string(),
            index_type: "flat".to_string(),
            nprobe: 10,
            ef_construction: 100,
            ef_search: 50,
        },
        db_config: DbConfig {
            url: "memory://test".to_string(),
            username: "root".to_string(),
            password: "root".to_string(),
            min_connections: 1,
            max_connections: 10,
            connect_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300),
        },
        cache_config: Some(CacheConfig {
            max_size: 1000,
            ttl: Duration::from_secs(300),
        }),
        pool_config: PoolConfig {
            min_size: 1,
            max_size: 10,
            timeout_seconds: 5,
            min_idle: 1,
            max_lifetime: Some(Duration::from_secs(3600)),
            idle_timeout: Some(Duration::from_secs(300)),
            connection_timeout: Duration::from_secs(5),
        },
        metrics_config: MetricsConfig {
            enabled: true,
            prefix: "test".to_string(),
            report_interval: Duration::from_secs(1),
        },
    }
}

async fn create_test_store() -> Box<dyn UnifiedStore> {
    create_unified_store(test_config()).await.unwrap()
}

fn test_vector_data() -> VectorData {
    VectorData {
        vector: (0..DIM).map(|i| i as f32).collect(),
        metadata: serde_json::json!({ "test_key": "test_value" }),
        dimension: DIM,
        namespace: "test_namespace".to_string(),
    }
}

fn test_scalar_data(index: i64) -> ScalarData {
    ScalarData {
        value: serde_json::json!({ "string_field": "test", "int_field": index }),
        metadata: serde_json::json!({ "collection": "test_collection" }),
    }
}

#[tokio::test]
async fn test_store_and_get_hybrid_roundtrip() {
    let store = create_test_store().await;
    let vector_data = test_vector_data();
    let scalar_data = test_scalar_data(42);

    let id = store
        .store_hybrid("test_key", &vector_data, &scalar_data)
        .await
        .unwrap();
    assert!(!id.is_nil());

    let retrieved: HybridData = store.get_hybrid("test_key").await.unwrap();
    assert_eq!(retrieved.vector, Some(vector_data.vector.clone()));
    assert_eq!(retrieved.value, scalar_data.value);
}

#[tokio::test]
async fn test_batch_store_hybrid() {
    let store = create_test_store().await;

    let records: Vec<HybridRecord> = (0..10)
        .map(|i| HybridRecord {
            key: format!("test_key_{}", i),
            data: HybridData {
                vector: Some(test_vector_data().vector),
                value: serde_json::json!({ "index": i }),
                metadata: serde_json::json!({}),
            },
        })
        .collect();

    let ids = store.batch_store_hybrid(&records).await.unwrap();
    assert_eq!(ids.len(), records.len());
}

#[tokio::test]
async fn test_put_get_json_roundtrip() {
    let store = create_test_store().await;

    // The string KV API (de)serializes a JSON `HybridData`; `put` persists it when a
    // vector is present, and `get` returns the serialized record.
    let data = HybridData {
        vector: Some((0..DIM).map(|i| i as f32).collect()),
        value: serde_json::json!({ "k": "v" }),
        metadata: serde_json::json!({}),
    };
    let json = serde_json::to_string(&data).unwrap();

    store.put("kv_key", &json).await.unwrap();
    assert!(store.get("kv_key").await.unwrap().is_some());
}

#[tokio::test]
async fn test_hybrid_search_returns_within_limit() {
    let store = create_test_store().await;

    for i in 0..20 {
        let scalar_data = test_scalar_data(i);
        store
            .store_hybrid(
                &format!("test_key_{}", i),
                &test_vector_data(),
                &scalar_data,
            )
            .await
            .unwrap();
    }

    let query: Vec<f32> = (0..DIM).map(|i| i as f32).collect();
    let filters = QueryFilters {
        metadata_filters: None,
        value_filters: None,
    };

    let results = store.hybrid_search(&query, &filters, 10).await.unwrap();
    assert!(results.len() <= 10);
    // Scores must be ordered best-first.
    for w in results.windows(2) {
        assert!(w[0].score >= w[1].score);
    }
}

#[tokio::test]
async fn test_streaming_search() {
    let store = create_test_store().await;
    for i in 0..20 {
        store
            .store_hybrid(
                &format!("stream_key_{}", i),
                &test_vector_data(),
                &test_scalar_data(i),
            )
            .await
            .unwrap();
    }

    let query: Vec<f32> = (0..DIM).map(|i| i as f32).collect();
    let filters = QueryFilters {
        metadata_filters: None,
        value_filters: None,
    };

    // `stream_hybrid_search` returns a boxed stream; pin it so `next()` is callable.
    let mut stream = Box::into_pin(
        store
            .stream_hybrid_search(&query, &filters, 10)
            .await
            .unwrap(),
    );
    while let Some(item) = stream.next().await {
        item.unwrap();
    }
}

#[tokio::test]
async fn test_get_missing_key_errors() {
    let store = create_test_store().await;
    assert!(store.get_hybrid("does_not_exist").await.is_err());
}
