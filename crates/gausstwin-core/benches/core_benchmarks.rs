//! Criterion benchmarks for the core simulation hot paths:
//! - the seeded random scheduler (activation ordering), and
//! - the end-to-end model step loop (agent execution).
//!
//! Run with `cargo bench -p gausstwin-core`.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use gausstwin_core::agent::{AgentFactory, DefaultAgentState};
use gausstwin_core::model::StandardModel;
use gausstwin_core::scheduler::{RandomScheduler, Scheduler};
use gausstwin_core::time::SimTime;
use gausstwin_core::{AgentId, Model, ModelConfig};

/// One full scheduling step: reshuffle and drain the activation order.
fn bench_scheduler(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduler_step");
    for &n in &[100usize, 1_000, 10_000] {
        let ids: Vec<AgentId> = (0..n as u128).map(AgentId::from_raw).collect();
        let mut sched = RandomScheduler::new(42);
        <RandomScheduler as Scheduler<DefaultAgentState>>::initialize(&mut sched, &ids).unwrap();

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                <RandomScheduler as Scheduler<DefaultAgentState>>::reset_step(&mut sched).unwrap();
                let mut activated = 0usize;
                while <RandomScheduler as Scheduler<DefaultAgentState>>::has_next(&sched) {
                    let batch = <RandomScheduler as Scheduler<DefaultAgentState>>::next_batch(
                        &mut sched,
                        SimTime::zero(),
                    )
                    .unwrap();
                    activated += batch.len();
                }
                black_box(activated)
            });
        });
    }
    group.finish();
}

/// End-to-end: run a model (build + initialize excluded from the timed section).
fn bench_model_run(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("model_run_10_ticks");
    for &n in &[100usize, 1_000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || {
                    rt.block_on(async {
                        let config = ModelConfig::new("bench".to_string())
                            .with_time_range(SimTime::zero(), SimTime::new(10.0))
                            .with_seed(42);
                        let mut model: StandardModel<DefaultAgentState> =
                            StandardModel::new(config).unwrap();
                        for _ in 0..n {
                            let agent =
                                AgentFactory::create_random_walker::<DefaultAgentState>(1.0);
                            model.add_agent(agent).await.unwrap();
                        }
                        model.initialize().await.unwrap();
                        model
                    })
                },
                |mut model| {
                    rt.block_on(async { model.run(None).await.unwrap() });
                    black_box(())
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_scheduler, bench_model_run);
criterion_main!(benches);
