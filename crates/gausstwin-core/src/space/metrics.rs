use super::Position;
use nalgebra::Vector3;

/// Calculate Euclidean distance between positions
pub fn euclidean_distance(p1: &Position, p2: &Position) -> f64 {
    match (p1, p2) {
        (Position::Grid(v1), Position::Grid(v2))
        | (Position::Continuous(v1), Position::Continuous(v2)) => {
            let diff = v1 - v2;
            diff.norm()
        }
        _ => f64::INFINITY,
    }
}

/// Calculate Manhattan distance between positions
pub fn manhattan_distance(p1: &Position, p2: &Position) -> f64 {
    match (p1, p2) {
        (Position::Grid(v1), Position::Grid(v2))
        | (Position::Continuous(v1), Position::Continuous(v2)) => {
            (v1.x - v2.x).abs() + (v1.y - v2.y).abs() + (v1.z - v2.z).abs()
        }
        _ => f64::INFINITY,
    }
}

/// Calculate Chebyshev distance between positions
pub fn chebyshev_distance(p1: &Position, p2: &Position) -> f64 {
    match (p1, p2) {
        (Position::Grid(v1), Position::Grid(v2))
        | (Position::Continuous(v1), Position::Continuous(v2)) => {
            let diff = v1 - v2;
            diff.abs().max()
        }
        _ => f64::INFINITY,
    }
}

/// Calculate Minkowski distance between positions
pub fn minkowski_distance(pos1: &Position, pos2: &Position, p: f64) -> f64 {
    match (pos1, pos2) {
        (Position::Grid(p1), Position::Grid(p2)) => {
            let diff_x = (p1.x - p2.x).abs().powf(p);
            let diff_y = (p1.y - p2.y).abs().powf(p);
            let diff_z = (p1.z - p2.z).abs().powf(p);
            (diff_x + diff_y + diff_z).powf(1.0 / p)
        }
        (Position::Continuous(p1), Position::Continuous(p2)) => {
            let diff_x = (p1.x - p2.x).abs().powf(p);
            let diff_y = (p1.y - p2.y).abs().powf(p);
            let diff_z = (p1.z - p2.z).abs().powf(p);
            (diff_x + diff_y + diff_z).powf(1.0 / p)
        }
        _ => panic!("Cannot calculate Minkowski distance between different position types"),
    }
}

/// Calculate normalized direction vector between positions
pub fn direction(pos1: &Position, pos2: &Position) -> Position {
    match (pos1, pos2) {
        (Position::Grid(p1), Position::Grid(p2)) => {
            let dir = p2 - p1;
            Position::Grid(dir)
        }
        (Position::Continuous(p1), Position::Continuous(p2)) => {
            let dir = p2 - p1;
            let length = dir.norm();
            if length > 0.0 {
                Position::Continuous(dir.normalize())
            } else {
                Position::Continuous(dir)
            }
        }
        _ => panic!("Cannot calculate direction between different position types"),
    }
}

pub fn direction_vector(from: &Position, to: &Position) -> Option<Position> {
    match (from, to) {
        (Position::Grid(v1), Position::Grid(v2))
        | (Position::Continuous(v1), Position::Continuous(v2)) => {
            let dir = v2 - v1;
            match from {
                Position::Grid(_) => Some(Position::Grid(dir)),
                Position::Continuous(_) => Some(Position::Continuous(dir)),
            }
        }
        _ => None,
    }
}
