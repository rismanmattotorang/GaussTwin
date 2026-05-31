use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use gausstwin_spaces::{
    common::{DistanceMetric, HighPerformanceMemoryPool},
    Point, SpatialIndex, VisualizationConfig,
};
use rand::{thread_rng, Rng};
use rayon::prelude::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const POINT_COUNTS: [usize; 4] = [1_000, 10_000, 100_000, 1_000_000];
const QUERY_RADII: [f64; 3] = [1.0, 5.0, 10.0];
const BATCH_SIZES: [usize; 3] = [100, 1000, 10000];

/// Generate random points within a cube
fn generate_random_points(count: usize, range: f64) -> Vec<(Point, usize)> {
    let mut rng = thread_rng();
    (0..count)
        .map(|i| {
            let point = Point::new(
                rng.gen_range(-range..range),
                rng.gen_range(-range..range),
                rng.gen_range(-range..range),
            );
            (point, i)
        })
        .collect()
}

/// Benchmark KD-tree operations
fn bench_kdtree(c: &mut Criterion) {
    let mut group = c.benchmark_group("kdtree");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    for &point_count in POINT_COUNTS.iter() {
        let points = generate_random_points(point_count, 100.0);

        // Insertion benchmarks
        group.throughput(Throughput::Elements(point_count as u64));
        group.bench_with_input(
            BenchmarkId::new("insert", point_count),
            &points,
            |b, points| {
                b.iter_batched(
                    || SpatialIndex::new_kdtree(),
                    |mut index| {
                        for (point, id) in points {
                            black_box(index.insert(*point, *id).unwrap());
                        }
                    },
                    BatchSize::LargeInput,
                )
            },
        );

        // Bulk insertion benchmarks
        group.bench_with_input(
            BenchmarkId::new("bulk_insert", point_count),
            &points,
            |b, points| {
                b.iter_batched(
                    || SpatialIndex::new_kdtree(),
                    |mut index| {
                        black_box(index.bulk_insert(points.clone()).unwrap());
                    },
                    BatchSize::LargeInput,
                )
            },
        );

        // Query benchmarks
        let index = SpatialIndex::new_kdtree();
        index.bulk_insert(points.clone()).unwrap();

        for &radius in QUERY_RADII.iter() {
            group.bench_with_input(
                BenchmarkId::new(format!("query_radius_{}", radius), point_count),
                &index,
                |b, index| {
                    let mut rng = thread_rng();
                    b.iter(|| {
                        let query_point = Point::new(
                            rng.gen_range(-100.0..100.0),
                            rng.gen_range(-100.0..100.0),
                            rng.gen_range(-100.0..100.0),
                        );
                        black_box(index.query_radius(query_point, radius))
                    })
                },
            );
        }
    }

    group.finish();
}

/// Benchmark grid hash operations
fn bench_grid_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_hash");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    for &point_count in POINT_COUNTS.iter() {
        let points = generate_random_points(point_count, 100.0);

        // Test different cell sizes
        for cell_size in [1.0, 2.0, 5.0].iter() {
            group.throughput(Throughput::Elements(point_count as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("insert_cell_size_{}", cell_size), point_count),
                &points,
                |b, points| {
                    b.iter_batched(
                        || SpatialIndex::new_grid_hash(*cell_size),
                        |mut index| {
                            for (point, id) in points {
                                black_box(index.insert(*point, *id).unwrap());
                            }
                        },
                        BatchSize::LargeInput,
                    )
                },
            );

            let index = SpatialIndex::new_grid_hash(*cell_size);
            index.bulk_insert(points.clone()).unwrap();

            for &radius in QUERY_RADII.iter() {
                group.bench_with_input(
                    BenchmarkId::new(
                        format!("query_radius_{}_cell_size_{}", radius, cell_size),
                        point_count,
                    ),
                    &index,
                    |b, index| {
                        let mut rng = thread_rng();
                        b.iter(|| {
                            let query_point = Point::new(
                                rng.gen_range(-100.0..100.0),
                                rng.gen_range(-100.0..100.0),
                                rng.gen_range(-100.0..100.0),
                            );
                            black_box(index.query_radius(query_point, radius))
                        })
                    },
                );
            }
        }
    }

    group.finish();
}

/// Benchmark R*-tree operations
fn bench_rtree(c: &mut Criterion) {
    let mut group = c.benchmark_group("rtree");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    for &point_count in POINT_COUNTS.iter() {
        let points = generate_random_points(point_count, 100.0);

        // Insertion benchmarks
        group.throughput(Throughput::Elements(point_count as u64));
        group.bench_with_input(
            BenchmarkId::new("insert", point_count),
            &points,
            |b, points| {
                b.iter_batched(
                    || SpatialIndex::new_rtree(),
                    |mut index| {
                        for (point, id) in points {
                            black_box(index.insert(*point, *id).unwrap());
                        }
                    },
                    BatchSize::LargeInput,
                )
            },
        );

        // Bulk insertion benchmarks
        group.bench_with_input(
            BenchmarkId::new("bulk_insert", point_count),
            &points,
            |b, points| {
                b.iter_batched(
                    || SpatialIndex::new_rtree(),
                    |mut index| {
                        black_box(index.bulk_insert(points.clone()).unwrap());
                    },
                    BatchSize::LargeInput,
                )
            },
        );

        let index = SpatialIndex::new_rtree();
        index.bulk_insert(points.clone()).unwrap();

        for &radius in QUERY_RADII.iter() {
            group.bench_with_input(
                BenchmarkId::new(format!("query_radius_{}", radius), point_count),
                &index,
                |b, index| {
                    let mut rng = thread_rng();
                    b.iter(|| {
                        let query_point = Point::new(
                            rng.gen_range(-100.0..100.0),
                            rng.gen_range(-100.0..100.0),
                            rng.gen_range(-100.0..100.0),
                        );
                        black_box(index.query_radius(query_point, radius))
                    })
                },
            );
        }
    }

    group.finish();
}

/// Benchmark octree operations
fn bench_octree(c: &mut Criterion) {
    let mut group = c.benchmark_group("octree");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    for &point_count in POINT_COUNTS.iter() {
        let points = generate_random_points(point_count, 100.0);

        // Insertion benchmarks
        group.throughput(Throughput::Elements(point_count as u64));
        group.bench_with_input(
            BenchmarkId::new("insert", point_count),
            &points,
            |b, points| {
                b.iter_batched(
                    || SpatialIndex::new_octree(Point::new(0.0, 0.0, 0.0), 200.0),
                    |mut index| {
                        for (point, id) in points {
                            black_box(index.insert(*point, *id).unwrap());
                        }
                    },
                    BatchSize::LargeInput,
                )
            },
        );

        let index = SpatialIndex::new_octree(Point::new(0.0, 0.0, 0.0), 200.0);
        index.bulk_insert(points.clone()).unwrap();

        for &radius in QUERY_RADII.iter() {
            group.bench_with_input(
                BenchmarkId::new(format!("query_radius_{}", radius), point_count),
                &index,
                |b, index| {
                    let mut rng = thread_rng();
                    b.iter(|| {
                        let query_point = Point::new(
                            rng.gen_range(-100.0..100.0),
                            rng.gen_range(-100.0..100.0),
                            rng.gen_range(-100.0..100.0),
                        );
                        black_box(index.query_radius(query_point, radius))
                    })
                },
            );
        }
    }

    group.finish();
}

/// Benchmark memory pool operations
fn bench_memory_pool(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pool");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    for &batch_size in BATCH_SIZES.iter() {
        group.throughput(Throughput::Elements(batch_size as u64));

        // Allocation benchmarks
        group.bench_with_input(
            BenchmarkId::new("allocate", batch_size),
            &batch_size,
            |b, &size| {
                let pool = HighPerformanceMemoryPool::<Vec<f64>>::new(size);
                b.iter(|| {
                    for _ in 0..size {
                        black_box(pool.allocate());
                    }
                })
            },
        );

        // Deallocation benchmarks
        group.bench_with_input(
            BenchmarkId::new("deallocate", batch_size),
            &batch_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let pool = HighPerformanceMemoryPool::<Vec<f64>>::new(size);
                        let values: Vec<_> = (0..size).map(|_| pool.allocate().unwrap()).collect();
                        (pool, values)
                    },
                    |(pool, values)| {
                        for value in values {
                            black_box(pool.deallocate(value));
                        }
                    },
                    BatchSize::LargeInput,
                )
            },
        );
    }

    group.finish();
}

/// Benchmark SIMD distance calculations
fn bench_simd_distance(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_distance");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    for &point_count in POINT_COUNTS.iter() {
        let points = generate_random_points(point_count, 100.0);
        let target = Point::new(0.0, 0.0, 0.0);

        // Scalar distance calculation
        group.bench_with_input(
            BenchmarkId::new("scalar", point_count),
            &points,
            |b, points| {
                b.iter(|| {
                    for (point, _) in points {
                        black_box(DistanceMetric::Euclidean.calculate(
                            point.x - target.x,
                            point.y - target.y,
                            point.z - target.z,
                        ));
                    }
                })
            },
        );

        // SIMD distance calculation
        group.bench_with_input(
            BenchmarkId::new("simd", point_count),
            &points,
            |b, points| {
                b.iter(|| {
                    for chunk in points.chunks(4) {
                        let mut x_vals = [0.0; 4];
                        let mut y_vals = [0.0; 4];
                        let mut z_vals = [0.0; 4];

                        for (i, (point, _)) in chunk.iter().enumerate() {
                            x_vals[i] = point.x - target.x;
                            y_vals[i] = point.y - target.y;
                            z_vals[i] = point.z - target.z;
                        }

                        unsafe {
                            let dx = f64x4::from_array(x_vals);
                            let dy = f64x4::from_array(y_vals);
                            let dz = f64x4::from_array(z_vals);
                            black_box(DistanceMetric::Euclidean.calculate_simd(dx, dy, dz));
                        }
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_kdtree,
    bench_grid_hash,
    bench_rtree,
    bench_octree,
    bench_memory_pool,
    bench_simd_distance,
);
criterion_main!(benches);
