use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gausstwin_des::{
    DiscreteEventSimulator, Event, EventData, MonitoringConfig, Priority, ResourceConstraint,
    RetryPolicy, SimulationConfig, ValidationRule,
};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

fn create_test_event(time: f64, priority: Priority) -> Event {
    Event {
        id: Uuid::new_v4(),
        time,
        priority,
        data: EventData::SystemEvent {
            event_type: "benchmark".into(),
            data: serde_json::json!({"test": true}),
            severity: "info".into(),
            correlation_id: None,
        },
        metadata: None,
        created_at: Utc::now(),
        dependencies: vec![],
        causality_chain: vec![],
        retry_count: 0,
        max_retries: 3,
        timeout: None,
        rollback_handler: None,
    }
}

fn create_test_config() -> SimulationConfig {
    SimulationConfig {
        max_time: 100.0,
        max_events: None,
        parallel_execution: true,
        max_concurrent_events: 4,
        checkpoint_interval: None,
        metrics_enabled: true,
        auto_scaling: true,
        retry_policy: RetryPolicy {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
        },
        optimization_enabled: true,
        resource_constraints: vec![ResourceConstraint {
            resource_type: "cpu".into(),
            max_capacity: 100.0,
            min_capacity: 10.0,
            scaling_factor: 1.5,
        }],
        validation_rules: vec![],
        monitoring_config: MonitoringConfig {
            metrics_interval: Duration::from_secs(10),
            alert_thresholds: HashMap::new(),
            log_level: "info".into(),
            tracing_enabled: false,
        },
    }
}

fn benchmark_sequential_scheduling(c: &mut Criterion) {
    let config = create_test_config();
    let simulator = DiscreteEventSimulator::new(config);

    c.bench_function("sequential_scheduling_100", |b| {
        b.iter(|| {
            for i in 0..100 {
                let event = create_test_event(i as f64, Priority::Normal);
                black_box(simulator.schedule_event(event).unwrap());
            }
        })
    });
}

fn benchmark_parallel_processing(c: &mut Criterion) {
    let mut config = create_test_config();
    config.parallel_execution = true;
    config.max_concurrent_events = 8;
    let simulator = DiscreteEventSimulator::new(config);

    c.bench_function("parallel_processing_1000", |b| {
        b.iter(|| {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                for i in 0..1000 {
                    let event = create_test_event(1.0, Priority::Normal);
                    black_box(simulator.schedule_event(event).unwrap());
                }
                black_box(simulator.run().await.unwrap());
            });
        })
    });
}

fn benchmark_mixed_priority_scheduling(c: &mut Criterion) {
    let config = create_test_config();
    let simulator = DiscreteEventSimulator::new(config);

    c.bench_function("mixed_priority_scheduling_500", |b| {
        b.iter(|| {
            for i in 0..500 {
                let priority = match i % 5 {
                    0 => Priority::Critical,
                    1 => Priority::High,
                    2 => Priority::Normal,
                    3 => Priority::Low,
                    _ => Priority::Background,
                };
                let event = create_test_event(i as f64, priority);
                black_box(simulator.schedule_event(event).unwrap());
            }
        })
    });
}

fn benchmark_resource_allocation(c: &mut Criterion) {
    let config = create_test_config();
    let simulator = DiscreteEventSimulator::new(config);

    c.bench_function("resource_allocation_200", |b| {
        b.iter(|| {
            for i in 0..200 {
                let event = Event {
                    id: Uuid::new_v4(),
                    time: i as f64,
                    priority: Priority::High,
                    data: EventData::ResourceAllocation {
                        resource_id: Uuid::new_v4(),
                        allocation_type: "cpu".into(),
                        quantity: 10.0,
                        constraints: vec!["max_capacity".into()],
                    },
                    metadata: None,
                    created_at: Utc::now(),
                    dependencies: vec![],
                    causality_chain: vec![],
                    retry_count: 0,
                    max_retries: 3,
                    timeout: Some(Duration::from_secs(5)),
                    rollback_handler: Some("deallocate_resource".into()),
                };
                black_box(simulator.schedule_event(event).unwrap());
            }
        })
    });
}

fn benchmark_optimization_events(c: &mut Criterion) {
    let mut config = create_test_config();
    config.optimization_enabled = true;
    let simulator = DiscreteEventSimulator::new(config);

    c.bench_function("optimization_events_100", |b| {
        b.iter(|| {
            for i in 0..100 {
                let event = Event {
                    id: Uuid::new_v4(),
                    time: i as f64,
                    priority: Priority::Normal,
                    data: EventData::Optimization {
                        objective: "minimize_latency".into(),
                        constraints: vec!["resource_limit".into()],
                        parameters: serde_json::json!({
                            "target": 100.0,
                            "tolerance": 0.01
                        }),
                        algorithm: "gradient_descent".into(),
                    },
                    metadata: None,
                    created_at: Utc::now(),
                    dependencies: vec![],
                    causality_chain: vec![],
                    retry_count: 0,
                    max_retries: 3,
                    timeout: None,
                    rollback_handler: None,
                };
                black_box(simulator.schedule_event(event).unwrap());
            }
        })
    });
}

criterion_group!(
    benches,
    benchmark_sequential_scheduling,
    benchmark_parallel_processing,
    benchmark_mixed_priority_scheduling,
    benchmark_resource_allocation,
    benchmark_optimization_events
);
criterion_main!(benches);
