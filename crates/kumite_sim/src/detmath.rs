//! Deterministic math for the simulation: the same bits on every platform (macOS, Windows, Linux, Android, iOS,
//! the browser). The standard library's `sin`, `cos`, `atan2` call each platform's own math library (or Rust's
//! for WebAssembly), whose last bits differ, and online play needs both sides' fights to stay identical; these go
//! through `libm` (pure Rust, IEEE basic operations only). glam uses it too (its `libm` feature) and does its
//! vector math without SIMD (`scalar-math`), so its sums add up in the same order everywhere.

pub trait DetMath: Sized + Copy {
    fn dsin(self) -> Self;
    fn dcos(self) -> Self;
    fn dsin_cos(self) -> (Self, Self) {
        (self.dsin(), self.dcos())
    }
    fn datan2(self, x: Self) -> Self;
}

impl DetMath for f32 {
    fn dsin(self) -> f32 {
        libm::sinf(self)
    }
    fn dcos(self) -> f32 {
        libm::cosf(self)
    }
    fn datan2(self, x: f32) -> f32 {
        libm::atan2f(self, x)
    }
}

impl DetMath for f64 {
    fn dsin(self) -> f64 {
        libm::sin(self)
    }
    fn dcos(self) -> f64 {
        libm::cos(self)
    }
    fn datan2(self, x: f64) -> f64 {
        libm::atan2(self, x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_values_as_the_standard_library_near_enough() {
        for k in -40..40 {
            let x = k as f32 * 0.173;
            assert!((x.dsin() - x.sin()).abs() < 1e-6 && (x.dcos() - x.cos()).abs() < 1e-6);
            assert!((x.datan2(1.3) - x.atan2(1.3)).abs() < 1e-6);
        }
    }
}
