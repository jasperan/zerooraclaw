//! Oracle AI Vector Search helpers.
//!
//! Utility functions for converting between Rust vectors and Oracle's
//! `TO_VECTOR()` string format, plus distance-to-similarity conversion.

use std::fmt::Write;

/// Convert a `&[f32]` slice into Oracle's `TO_VECTOR()` compatible string.
///
/// Output format: `[0.1, 0.2, -0.3, ...]`
///
/// This string is passed as a bind parameter to `TO_VECTOR(:vec, 384, FLOAT32)`.
pub fn vec_to_oracle_string(v: &[f32]) -> String {
    let mut buf = String::with_capacity(v.len() * 10 + 2);
    buf.push('[');
    for (i, val) in v.iter().enumerate() {
        if i > 0 {
            buf.push(',');
        }
        // Use enough precision to preserve f32 round-trip fidelity
        let _ = write!(buf, "{val}");
    }
    buf.push(']');
    buf
}

/// Convert a cosine distance (0.0 = identical, 2.0 = opposite) to a similarity
/// score in the range `[0.0, 1.0]`.
///
/// Formula: `max(1.0 - distance, 0.0)`
pub fn similarity_from_distance(distance: f64) -> f64 {
    (1.0 - distance).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_to_oracle_string_empty() {
        assert_eq!(vec_to_oracle_string(&[]), "[]");
    }

    #[test]
    fn vec_to_oracle_string_single() {
        let s = vec_to_oracle_string(&[1.5]);
        assert_eq!(s, "[1.5]");
    }

    #[test]
    fn vec_to_oracle_string_multiple() {
        let s = vec_to_oracle_string(&[0.1, -0.2, 3.0]);
        assert_eq!(s, "[0.1,-0.2,3]");
    }

    #[test]
    fn vec_to_oracle_string_preserves_negatives() {
        let s = vec_to_oracle_string(&[-1.0, -0.5]);
        assert!(s.starts_with("[-1"));
        assert!(s.contains("-0.5"));
    }

    #[test]
    fn similarity_identical() {
        assert!((similarity_from_distance(0.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_half() {
        assert!((similarity_from_distance(0.5) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_opposite() {
        assert!((similarity_from_distance(1.0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_clamps_negative() {
        // distance > 1.0 should still return 0.0 (clamped)
        assert!((similarity_from_distance(1.5) - 0.0).abs() < f64::EPSILON);
        assert!((similarity_from_distance(2.0) - 0.0).abs() < f64::EPSILON);
    }
}
