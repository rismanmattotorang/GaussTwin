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
    // Both `Position` variants wrap a 3D coordinate vector, so compute from the
    // coordinates directly. (Previously this panicked when the two positions were of
    // different variants — an input-driven panic.)
    let (v1, v2) = (pos1.coords(), pos2.coords());
    let diff_x = (v1.x - v2.x).abs().powf(p);
    let diff_y = (v1.y - v2.y).abs().powf(p);
    let diff_z = (v1.z - v2.z).abs().powf(p);
    (diff_x + diff_y + diff_z).powf(1.0 / p)
}

/// Calculate normalized direction vector between positions
pub fn direction(pos1: &Position, pos2: &Position) -> Position {
    // Compute from coordinates so mismatched variants don't panic; the output
    // variant and normalization follow `pos1`'s kind (grid = raw delta, continuous
    // = unit vector), matching the original same-variant behavior.
    let dir = pos2.coords() - pos1.coords();
    match pos1 {
        Position::Grid(_) => Position::Grid(dir),
        Position::Continuous(_) => {
            let length = dir.norm();
            if length > 0.0 {
                Position::Continuous(dir.normalize())
            } else {
                Position::Continuous(dir)
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    // Regression: mixed `Position` variants must not panic (they used to).
    #[test]
    fn mixed_variants_do_not_panic() {
        let grid = Position::Grid(Vector3::new(0.0, 0.0, 0.0));
        let cont = Position::Continuous(Vector3::new(3.0, 4.0, 0.0));

        let d = minkowski_distance(&grid, &cont, 2.0);
        assert!(d.is_finite());
        assert!((d - 5.0).abs() < 1e-9); // 3-4-5 triangle

        // Direction across variants follows pos1's kind and does not panic.
        let dir = direction(&grid, &cont);
        assert!(matches!(dir, Position::Grid(_)));
    }
}
