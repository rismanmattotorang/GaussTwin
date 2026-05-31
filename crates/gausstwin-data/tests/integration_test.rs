use futures::StreamExt;
use gausstwin_data::{
    create_unified_store, CacheConfig, DbConfig, FilterCondition, FilterOperator, HybridRecord,
    MetricsConfig, PoolConfig, QueryFilters, ScalarData, UnifiedStore, UnifiedStoreConfig, Value,
    VectorData, VectorStoreConfig,
};
use ndarray::Array1;
use std::time::Duration;
use tokio;

async fn create_test_store() -> Box<dyn UnifiedStore> {
    let config = UnifiedStoreConfig {
        vector_config: VectorStoreConfig {
            url: "memory://test".to_string(),
            dimension: 128,
            index_type: "flat".to_string(),
            metric_type: "l2".to_string(),
            nprobe: 10,
            ef_construction: 100,
            ef_search: 50,
        },
        db_config: DbConfig {
            url: "memory://test".to_string(),
            max_connections: 10,
            min_connections: 2,
            connect_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300),
        },
        cache_config: Some(CacheConfig {
            max_size: 1000,
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
            prefix: "test".to_string(),
            report_interval: Duration::from_secs(1),
        },
    };

    create_unified_store(config).await.unwrap()
}

fn create_test_vector_data(dimension: usize) -> VectorData {
    let vector = Array1::from_vec((0..dimension).map(|i| i as f32).collect());
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("test_key".to_string(), "test_value".to_string());

    VectorData {
        vector,
        metadata,
        dimension,
        namespace: Some("test_namespace".to_string()),
    }
}

fn create_test_scalar_data() -> ScalarData {
    let mut fields = std::collections::HashMap::new();
    fields.insert(
        "string_field".to_string(),
        Value::String("test".to_string()),
    );
    fields.insert("int_field".to_string(), Value::Integer(42));
    fields.insert("float_field".to_string(), Value::Float(3.14));

    ScalarData {
        fields,
        schema_version: 1,
        collection: Some("test_collection".to_string()),
    }
}

#[tokio::test]
async fn test_basic_operations() {
    let store = create_test_store().await;

    // Test store operation
    let vector_data = create_test_vector_data(128);
    let scalar_data = create_test_scalar_data();

    let id = store
        .store_hybrid("test_key", &vector_data, &scalar_data)
        .await
        .unwrap();
    assert!(!id.is_nil());

    // Test retrieve operation
    let retrieved = store.get_hybrid("test_key").await.unwrap();
    assert_eq!(retrieved.id, id);
    assert_eq!(retrieved.vector.dimension, vector_data.dimension);
    assert_eq!(retrieved.scalar.schema_version, scalar_data.schema_version);

    // Verify vector data
    assert_eq!(retrieved.vector.vector, vector_data.vector);
    assert_eq!(retrieved.vector.metadata, vector_data.metadata);
    assert_eq!(retrieved.vector.namespace, vector_data.namespace);

    // Verify scalar data
    assert_eq!(retrieved.scalar.fields.len(), scalar_data.fields.len());
    assert_eq!(retrieved.scalar.collection, scalar_data.collection);
}

#[tokio::test]
async fn test_batch_operations() {
    let store = create_test_store().await;

    // Create test records
    let mut records = Vec::new();
    for i in 0..10 {
        let vector_data = create_test_vector_data(128);
        let mut scalar_data = create_test_scalar_data();
        scalar_data
            .fields
            .insert("index".to_string(), Value::Integer(i));

        records.push(HybridRecord {
            key: format!("test_key_{}", i),
            vector_data,
            scalar_data,
        });
    }

    // Test batch store
    let ids = store.batch_store_hybrid(&records).await.unwrap();
    assert_eq!(ids.len(), records.len());

    // Verify each record
    for (i, id) in ids.iter().enumerate() {
        let retrieved = store.get_hybrid(&format!("test_key_{}", i)).await.unwrap();
        assert_eq!(retrieved.id, *id);
    }
}

#[tokio::test]
async fn test_hybrid_search() {
    let store = create_test_store().await;

    // Store test data
    for i in 0..100 {
        let vector_data = create_test_vector_data(128);
        let mut scalar_data = create_test_scalar_data();
        scalar_data
            .fields
            .insert("index".to_string(), Value::Integer(i));

        store
            .store_hybrid(&format!("test_key_{}", i), &vector_data, &scalar_data)
            .await
            .unwrap();
    }

    // Create search query
    let query_vector = Array1::from_vec((0..128).map(|i| i as f32).collect());
    let scalar_filters = QueryFilters {
        conditions: vec![FilterCondition {
            field: "int_field".to_string(),
            operator: crate::ComparisonOperator::Eq,
            value: Value::Integer(42),
        }],
        combine_operator: FilterOperator::And,
    };

    // Test search
    let results = store
        .hybrid_search(&query_vector.as_slice().unwrap(), &scalar_filters, 10)
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert!(results.len() <= 10);

    // Verify results are sorted by combined score
    for i in 1..results.len() {
        assert!(results[i - 1].combined_score >= results[i].combined_score);
    }
}

#[tokio::test]
async fn test_streaming_search() {
    let store = create_test_store().await;

    // Store test data
    for i in 0..100 {
        let vector_data = create_test_vector_data(128);
        let mut scalar_data = create_test_scalar_data();
        scalar_data
            .fields
            .insert("index".to_string(), Value::Integer(i));

        store
            .store_hybrid(&format!("test_key_{}", i), &vector_data, &scalar_data)
            .await
            .unwrap();
    }

    // Create search query
    let query_vector = Array1::from_vec((0..128).map(|i| i as f32).collect());
    let scalar_filters = QueryFilters {
        conditions: vec![FilterCondition {
            field: "int_field".to_string(),
            operator: crate::ComparisonOperator::Eq,
            value: Value::Integer(42),
        }],
        combine_operator: FilterOperator::And,
    };

    // Test streaming search
    let mut stream = store
        .stream_hybrid_search(&query_vector.as_slice().unwrap(), &scalar_filters, 10)
        .await
        .unwrap();

    let mut results = Vec::new();
    while let Some(result) = stream.next().await {
        results.push(result.unwrap());
    }

    assert!(!results.is_empty());

    // Verify results are sorted by combined score
    for i in 1..results.len() {
        assert!(results[i - 1].combined_score >= results[i].combined_score);
    }
}

#[tokio::test]
async fn test_cache_behavior() {
    let store = create_test_store().await;

    let vector_data = create_test_vector_data(128);
    let scalar_data = create_test_scalar_data();

    // Store data
    store
        .store_hybrid("cache_test", &vector_data, &scalar_data)
        .await
        .unwrap();

    // First retrieval (should miss cache)
    let start = std::time::Instant::now();
    let _ = store.get_hybrid("cache_test").await.unwrap();
    let first_duration = start.elapsed();

    // Second retrieval (should hit cache)
    let start = std::time::Instant::now();
    let _ = store.get_hybrid("cache_test").await.unwrap();
    let second_duration = start.elapsed();

    // Cache hit should be significantly faster
    assert!(second_duration < first_duration);
}

#[tokio::test]
async fn test_error_handling() {
    let store = create_test_store().await;

    // Test invalid vector dimension
    let invalid_vector_data = create_test_vector_data(64); // Wrong dimension
    let scalar_data = create_test_scalar_data();

    let result = store
        .store_hybrid("error_test", &invalid_vector_data, &scalar_data)
        .await;
    assert!(result.is_err());

    // Test non-existent key
    let result = store.get_hybrid("non_existent_key").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_concurrent_operations() {
    let store = create_test_store().await;
    let store = std::sync::Arc::new(store);

    let mut handles = Vec::new();

    // Spawn multiple concurrent operations
    for i in 0..10 {
        let store_clone = store.clone();
        let handle = tokio::spawn(async move {
            let vector_data = create_test_vector_data(128);
            let mut scalar_data = create_test_scalar_data();
            scalar_data
                .fields
                .insert("thread_id".to_string(), Value::Integer(i));

            let key = format!("concurrent_test_{}", i);
            let id = store_clone
                .store_hybrid(&key, &vector_data, &scalar_data)
                .await
                .unwrap();

            // Immediately try to retrieve
            let retrieved = store_clone.get_hybrid(&key).await.unwrap();
            assert_eq!(retrieved.id, id);
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }
}
