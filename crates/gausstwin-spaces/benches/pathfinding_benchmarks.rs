use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gausstwin_spaces::pathfinding::{
    algorithms::{AStar, AStarConfig, RRTConfig, RRT},
    traits::{CollisionChecker, CostFunction, PathFinder},
    Point,
};
use nalgebra::Vector3;
use rand::prelude::*;
use std::sync::Arc;

// Mock collision checker that creates random obstacles
struct BenchmarkCollisionChecker {
    obstacles: Vec<Point>,
    rng: ThreadRng,
}

impl BenchmarkCollisionChecker {
    fn new(num_obstacles: usize) -> Self {
        let mut rng = thread_rng();
        let mut obstacles = Vec::with_capacity(num_obstacles);

        for _ in 0..num_obstacles {
            let x = rng.gen_range(-10.0..10.0);
            let y = rng.gen_range(-10.0..10.0);
            let z = rng.gen_range(-10.0..10.0);
            obstacles.push(Point::new(x, y, z));
        }

        Self { obstacles, rng }
    }
}

impl CollisionChecker for BenchmarkCollisionChecker {
    fn is_in_collision(&self, point: &Point) -> bool {
        self.obstacles.iter().any(|obstacle| {
            (obstacle.coords - point.coords).norm() < 0.5 // Obstacle radius
        })
    }

    fn line_of_sight(&self, from: &Point, to: &Point) -> bool {
        let direction = to.coords - from.coords;
        let distance = direction.norm();
        let steps = (distance / 0.1).ceil() as usize;

        if steps == 0 {
            return true;
        }

        let step_vector = direction / steps as f64;

        (0..=steps).all(|i| {
            let point = Point::from(from.coords + step_vector * i as f64);
            !self.is_in_collision(&point)
        })
    }

    fn distance_to_obstacle(&self, point: &Point) -> f64 {
        self.obstacles
            .iter()
            .map(|obstacle| (obstacle.coords - point.coords).norm() - 0.5)
            .fold(f64::INFINITY, f64::min)
    }
}

// Euclidean distance cost function
struct BenchmarkCostFunction;

impl CostFunction for BenchmarkCostFunction {
    fn cost(&self, from: &Point, to: &Point) -> f64 {
        (to.coords - from.coords).norm()
    }

    fn heuristic(&self, from: &Point, to: &Point) -> f64 {
        (to.coords - from.coords).norm()
    }
}

// Generate random test cases
fn generate_test_cases(num_cases: usize) -> Vec<(Point, Point)> {
    let mut rng = thread_rng();
    let mut cases = Vec::with_capacity(num_cases);

    for _ in 0..num_cases {
        let start = Point::new(
            rng.gen_range(-10.0..10.0),
            rng.gen_range(-10.0..10.0),
            rng.gen_range(-10.0..10.0),
        );
        let goal = Point::new(
            rng.gen_range(-10.0..10.0),
            rng.gen_range(-10.0..10.0),
            rng.gen_range(-10.0..10.0),
        );
        cases.push((start, goal));
    }

    cases
}

fn benchmark_astar(c: &mut Criterion) {
    let collision_checker = Arc::new(BenchmarkCollisionChecker::new(100));
    let cost_function = Arc::new(BenchmarkCostFunction);
    let config = AStarConfig::default();
    let astar = AStar::new(collision_checker, cost_function, config);

    let test_cases = generate_test_cases(10);

    c.bench_function("astar_pathfinding", |b| {
        b.iter(|| {
            for (start, goal) in test_cases.iter() {
                black_box(astar.find_path(*start, *goal));
            }
        })
    });
}

fn benchmark_rrt(c: &mut Criterion) {
    let collision_checker = Arc::new(BenchmarkCollisionChecker::new(100));
    let cost_function = Arc::new(BenchmarkCostFunction);
    let config = RRTConfig::default();
    let rrt = RRT::new(collision_checker, cost_function, config);

    let test_cases = generate_test_cases(10);

    c.bench_function("rrt_pathfinding", |b| {
        b.iter(|| {
            for (start, goal) in test_cases.iter() {
                black_box(rrt.find_path(*start, *goal));
            }
        })
    });
}

fn benchmark_parallel_vs_sequential(c: &mut Criterion) {
    let collision_checker = Arc::new(BenchmarkCollisionChecker::new(100));
    let cost_function = Arc::new(BenchmarkCostFunction);

    let mut parallel_config = AStarConfig::default();
    parallel_config.use_parallel = true;
    let parallel_astar = AStar::new(
        collision_checker.clone(),
        cost_function.clone(),
        parallel_config,
    );

    let mut sequential_config = AStarConfig::default();
    sequential_config.use_parallel = false;
    let sequential_astar = AStar::new(collision_checker, cost_function, sequential_config);

    let test_cases = generate_test_cases(10);

    let mut group = c.benchmark_group("parallel_vs_sequential");
    group.bench_function("parallel_astar", |b| {
        b.iter(|| {
            for (start, goal) in test_cases.iter() {
                black_box(parallel_astar.find_path(*start, *goal));
            }
        })
    });
    group.bench_function("sequential_astar", |b| {
        b.iter(|| {
            for (start, goal) in test_cases.iter() {
                black_box(sequential_astar.find_path(*start, *goal));
            }
        })
    });
    group.finish();
}

fn benchmark_cache_impact(c: &mut Criterion) {
    let collision_checker = Arc::new(BenchmarkCollisionChecker::new(100));
    let cost_function = Arc::new(BenchmarkCostFunction);

    let mut cached_config = AStarConfig::default();
    cached_config.cache_neighbors = true;
    cached_config.cache_distances = true;
    let cached_astar = AStar::new(
        collision_checker.clone(),
        cost_function.clone(),
        cached_config,
    );

    let mut uncached_config = AStarConfig::default();
    uncached_config.cache_neighbors = false;
    uncached_config.cache_distances = false;
    let uncached_astar = AStar::new(collision_checker, cost_function, uncached_config);

    let test_cases = generate_test_cases(10);

    let mut group = c.benchmark_group("cache_impact");
    group.bench_function("cached_astar", |b| {
        b.iter(|| {
            for (start, goal) in test_cases.iter() {
                black_box(cached_astar.find_path(*start, *goal));
            }
        })
    });
    group.bench_function("uncached_astar", |b| {
        b.iter(|| {
            for (start, goal) in test_cases.iter() {
                black_box(uncached_astar.find_path(*start, *goal));
            }
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    benchmark_astar,
    benchmark_rrt,
    benchmark_parallel_vs_sequential,
    benchmark_cache_impact
);
criterion_main!(benches);
