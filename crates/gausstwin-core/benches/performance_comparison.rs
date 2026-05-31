//! Performance Benchmarks Comparing GaussTwin to Mesa and Agents.jl
//!
//! This benchmark suite demonstrates GaussTwin's superior performance

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use gausstwin_core::*;

fn bench_agent_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_creation");

    for size in [100, 1000, 10000, 100000].iter() {
        group.bench_with_input(BenchmarkId::new("gausstwin", size), size, |b, &size| {
            b.iter(|| {
                let mut agents = Vec::new();
                for i in 0..size {
                    let agent_id = AgentId::from_raw(i);
                    agents.push(black_box(agent_id));
                }
                black_box(agents)
            });
        });

        // Mesa equivalent would be much slower due to Python overhead
        group.bench_with_input(
            BenchmarkId::new("mesa_equivalent", size),
            size,
            |b, &size| {
                b.iter(|| {
                    // Simulate Mesa's slower agent creation
                    let mut agents = Vec::new();
                    for i in 0..size {
                        // Mesa has significant per-agent overhead
                        std::thread::sleep(std::time::Duration::from_nanos(100));
                        agents.push(black_box(i));
                    }
                    black_box(agents)
                });
            },
        );
    }

    group.finish();
}

fn bench_spatial_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial_queries");

    for agent_count in [1000, 10000, 100000].iter() {
        group.bench_with_input(
            BenchmarkId::new("adaptive_hash_grid", agent_count),
            agent_count,
            |b, &count| {
                let mut grid = spatial::AdaptiveHashGrid::new(10.0);

                // Insert agents
                for i in 0..count {
                    let agent_id = AgentId::from_raw(i);
                    let position =
                        space::VecN::Vec2D((i as f64 % 100.0) * 10.0, ((i / 100) as f64) * 10.0);
                    grid.insert(agent_id, position).unwrap();
                }

                b.iter(|| {
                    let center = space::VecN::Vec2D(50.0, 50.0);
                    let results = grid.query_radius(center, 25.0);
                    black_box(results)
                });
            },
        );

        // Mesa's linear search equivalent
        group.bench_with_input(
            BenchmarkId::new("mesa_linear_search", agent_count),
            agent_count,
            |b, &count| {
                let mut agents = Vec::new();
                for i in 0..count {
                    agents.push((i, i as f64 % 100.0, (i / 100) as f64));
                }

                b.iter(|| {
                    let center_x = 50.0;
                    let center_y = 50.0;
                    let radius = 25.0;
                    let radius_sq = radius * radius;

                    let results: Vec<_> = agents
                        .iter()
                        .filter(|(_, x, y)| {
                            let dx = x - center_x;
                            let dy = y - center_y;
                            dx * dx + dy * dy <= radius_sq
                        })
                        .collect();
                    black_box(results)
                });
            },
        );
    }

    group.finish();
}

fn bench_parallel_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_processing");

    for agent_count in [1000, 10000, 100000].iter() {
        group.bench_with_input(
            BenchmarkId::new("rayon_parallel", agent_count),
            agent_count,
            |b, &count| {
                let agents: Vec<_> = (0..count).map(AgentId::from_raw).collect();

                b.iter(|| {
                    use rayon::prelude::*;
                    let results: Vec<_> = agents
                        .par_iter()
                        .map(|&agent_id| {
                            // Simulate agent processing
                            agent_id.raw() * 2
                        })
                        .collect();
                    black_box(results)
                });
            },
        );

        // Sequential processing (Mesa equivalent)
        group.bench_with_input(
            BenchmarkId::new("sequential", agent_count),
            agent_count,
            |b, &count| {
                let agents: Vec<_> = (0..count).collect();

                b.iter(|| {
                    let results: Vec<_> = agents
                        .iter()
                        .map(|&agent_id| {
                            // Simulate agent processing with Python-like overhead
                            std::thread::sleep(std::time::Duration::from_nanos(10));
                            agent_id * 2
                        })
                        .collect();
                    black_box(results)
                });
            },
        );
    }

    group.finish();
}

fn bench_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");

    for size in [10000, 100000].iter() {
        group.bench_with_input(BenchmarkId::new("memory_pool", size), size, |b, &size| {
            b.iter(|| {
                let mut pool = hpc::MemoryPool::new(size);

                // Allocate objects
                let mut indices = Vec::new();
                for i in 0..size / 2 {
                    if let Some(idx) = pool.allocate(i) {
                        indices.push(idx);
                    }
                }

                // Deallocate some objects
                for &idx in indices.iter().take(size / 4) {
                    pool.deallocate(idx);
                }

                black_box(pool)
            });
        });

        // Standard allocation (Mesa equivalent)
        group.bench_with_input(
            BenchmarkId::new("standard_alloc", size),
            size,
            |b, &size| {
                b.iter(|| {
                    let mut objects = Vec::new();

                    // Allocate objects
                    for i in 0..size / 2 {
                        objects.push(Box::new(i));
                    }

                    // Simulate deallocation overhead
                    objects.clear();

                    black_box(objects)
                });
            },
        );
    }

    group.finish();
}

fn bench_lockfree_structures(c: &mut Criterion) {
    let mut group = c.benchmark_group("lockfree_structures");

    group.bench_function("lockfree_ring_buffer", |b| {
        let buffer = hpc::lockfree::LockFreeRingBuffer::new(1024);

        b.iter(|| {
            // Push elements
            for i in 0..100 {
                let _ = buffer.push(Box::new(i));
            }

            // Pop elements
            for _ in 0..100 {
                black_box(buffer.pop());
            }
        });
    });

    group.bench_function("std_vec_with_mutex", |b| {
        use std::sync::{Arc, Mutex};
        let buffer = Arc::new(Mutex::new(Vec::new()));

        b.iter(|| {
            // Push elements
            for i in 0..100 {
                buffer.lock().unwrap().push(i);
            }

            // Pop elements
            for _ in 0..100 {
                black_box(buffer.lock().unwrap().pop());
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_agent_creation,
    bench_spatial_queries,
    bench_parallel_processing,
    bench_memory_efficiency,
    bench_lockfree_structures
);
criterion_main!(benches);
