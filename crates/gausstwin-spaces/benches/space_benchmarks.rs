#![feature(test)]
extern crate test;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gausstwin_spaces::{
    continuous::{ContinuousSpace, ContinuousSpaceConfig},
    graph::{EdgeWeight, GraphSpace, GraphSpaceConfig, GraphType},
    grid::{GridPosition, GridSpace, GridSpaceConfig},
    AgentId, Space,
};
use nalgebra::Point3;
use rand::{thread_rng, Rng};
use std::time::Duration;

fn generate_random_point() -> Point3<f64> {
    let mut rng = thread_rng();
    Point3::new(
        rng.gen_range(-100.0..100.0),
        rng.gen_range(-100.0..100.0),
        rng.gen_range(-100.0..100.0),
    )
}

fn generate_random_grid_position() -> GridPosition {
    let mut rng = thread_rng();
    GridPosition::new(
        rng.gen_range(0..100),
        rng.gen_range(0..100),
        rng.gen_range(0..100),
    )
}

fn bench_continuous_space(c: &mut Criterion) {
    let mut group = c.benchmark_group("continuous_space");
    group.measurement_time(Duration::from_secs(10));

    let space = ContinuousSpace::new(ContinuousSpaceConfig::default());
    let mut agents = Vec::new();

    // Setup: Add 10,000 agents
    for _ in 0..10_000 {
        let id = AgentId::new();
        let pos = generate_random_point();
        space.add_agent(id, pos);
        agents.push((id, pos));
    }

    group.bench_function("add_agent", |b| {
        b.iter(|| {
            let id = AgentId::new();
            let pos = generate_random_point();
            space.add_agent(black_box(id), black_box(pos));
        });
    });

    group.bench_function("remove_agent", |b| {
        b.iter(|| {
            if let Some((id, _)) = agents.last() {
                space.remove_agent(black_box(*id));
            }
        });
    });

    group.bench_function("move_agent", |b| {
        b.iter(|| {
            if let Some((id, _)) = agents.first() {
                let new_pos = generate_random_point();
                space.move_agent(black_box(*id), black_box(new_pos));
            }
        });
    });

    group.bench_function("query_radius", |b| {
        b.iter(|| {
            let center = generate_random_point();
            space.query_radius(black_box(center), black_box(10.0));
        });
    });

    group.bench_function("query_k_nearest", |b| {
        b.iter(|| {
            let center = generate_random_point();
            space.query_k_nearest(black_box(center), black_box(10));
        });
    });

    group.finish();
}

fn bench_grid_space(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_space");
    group.measurement_time(Duration::from_secs(10));

    let space = GridSpace::new(GridSpaceConfig::default());
    let mut agents = Vec::new();

    // Setup: Add 10,000 agents
    for _ in 0..10_000 {
        let id = AgentId::new();
        let pos = generate_random_grid_position();
        space.add_agent(id, pos);
        agents.push((id, pos));
    }

    group.bench_function("add_agent", |b| {
        b.iter(|| {
            let id = AgentId::new();
            let pos = generate_random_grid_position();
            space.add_agent(black_box(id), black_box(pos));
        });
    });

    group.bench_function("remove_agent", |b| {
        b.iter(|| {
            if let Some((id, _)) = agents.last() {
                space.remove_agent(black_box(*id));
            }
        });
    });

    group.bench_function("move_agent", |b| {
        b.iter(|| {
            if let Some((id, _)) = agents.first() {
                let new_pos = generate_random_grid_position();
                space.move_agent(black_box(*id), black_box(new_pos));
            }
        });
    });

    group.bench_function("query_radius", |b| {
        b.iter(|| {
            let center = generate_random_grid_position();
            space.query_radius(black_box(center), black_box(5.0));
        });
    });

    group.bench_function("query_k_nearest", |b| {
        b.iter(|| {
            let center = generate_random_grid_position();
            space.query_k_nearest(black_box(center), black_box(10));
        });
    });

    group.bench_function("find_path", |b| {
        b.iter(|| {
            if let Some((id1, _)) = agents.first() {
                if let Some((id2, _)) = agents.last() {
                    space.find_path(black_box(*id1), black_box(*id2));
                }
            }
        });
    });

    group.finish();
}

fn bench_graph_space(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_space");
    group.measurement_time(Duration::from_secs(10));

    let mut config = GraphSpaceConfig::default();
    config.graph_type = GraphType::Undirected;
    config.weighted = true;
    config.use_communities = true;

    let space = GraphSpace::new(config);
    let mut agents = Vec::new();

    // Setup: Add 1,000 agents and some edges
    for _ in 0..1_000 {
        let id = AgentId::new();
        let pos = generate_random_point();
        space.add_agent(id, pos);
        agents.push((id, pos));
    }

    // Add some random edges
    for i in 0..agents.len() {
        if i > 0 {
            let weight = EdgeWeight::default();
            let _ = space.add_edge(agents[i - 1].0, agents[i].0, weight.clone());
        }
    }

    group.bench_function("add_agent", |b| {
        b.iter(|| {
            let id = AgentId::new();
            let pos = generate_random_point();
            space.add_agent(black_box(id), black_box(pos));
        });
    });

    group.bench_function("remove_agent", |b| {
        b.iter(|| {
            if let Some((id, _)) = agents.last() {
                space.remove_agent(black_box(*id));
            }
        });
    });

    group.bench_function("move_agent", |b| {
        b.iter(|| {
            if let Some((id, _)) = agents.first() {
                let new_pos = generate_random_point();
                space.move_agent(black_box(*id), black_box(new_pos));
            }
        });
    });

    group.bench_function("add_edge", |b| {
        b.iter(|| {
            if let Some((id1, _)) = agents.first() {
                if let Some((id2, _)) = agents.last() {
                    let weight = EdgeWeight::default();
                    let _ = space.add_edge(black_box(*id1), black_box(*id2), black_box(weight));
                }
            }
        });
    });

    group.bench_function("query_radius", |b| {
        b.iter(|| {
            let center = generate_random_point();
            space.query_radius(black_box(center), black_box(10.0));
        });
    });

    group.bench_function("query_k_nearest", |b| {
        b.iter(|| {
            let center = generate_random_point();
            space.query_k_nearest(black_box(center), black_box(10));
        });
    });

    group.bench_function("find_path", |b| {
        b.iter(|| {
            if let Some((id1, _)) = agents.first() {
                if let Some((id2, _)) = agents.last() {
                    space.find_path(black_box(*id1), black_box(*id2));
                }
            }
        });
    });

    group.bench_function("detect_communities", |b| {
        b.iter(|| {
            let _ = space.detect_communities();
        });
    });

    group.finish();
}

fn bench_spatial_indexing(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial_indexing");
    group.measurement_time(Duration::from_secs(10));

    // Test different spatial indexing strategies
    let mut config = ContinuousSpaceConfig::default();

    // Grid-based indexing
    config.cell_size = Some(10.0);
    let grid_space = ContinuousSpace::new(config.clone());

    // KD-tree indexing
    config.cell_size = None;
    let kdtree_space = ContinuousSpace::new(config);

    // Setup: Add 10,000 agents to each space
    let mut agents = Vec::new();
    for _ in 0..10_000 {
        let id = AgentId::new();
        let pos = generate_random_point();
        grid_space.add_agent(id, pos.clone());
        kdtree_space.add_agent(id, pos.clone());
        agents.push((id, pos));
    }

    group.bench_function("grid_radius_query", |b| {
        b.iter(|| {
            let center = generate_random_point();
            grid_space.query_radius(black_box(center), black_box(10.0));
        });
    });

    group.bench_function("kdtree_radius_query", |b| {
        b.iter(|| {
            let center = generate_random_point();
            kdtree_space.query_radius(black_box(center), black_box(10.0));
        });
    });

    group.bench_function("grid_k_nearest", |b| {
        b.iter(|| {
            let center = generate_random_point();
            grid_space.query_k_nearest(black_box(center), black_box(10));
        });
    });

    group.bench_function("kdtree_k_nearest", |b| {
        b.iter(|| {
            let center = generate_random_point();
            kdtree_space.query_k_nearest(black_box(center), black_box(10));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_continuous_space,
    bench_grid_space,
    bench_graph_space,
    bench_spatial_indexing,
);
criterion_main!(benches);
