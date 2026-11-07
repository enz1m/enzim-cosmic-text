// Modified by enzUI contributors. See FORK.md.
#[cfg(not(feature = "std"))]
pub use libm::{floorf, roundf};

#[cfg(feature = "std")]
#[inline]
pub fn floorf(x: f32) -> f32 {
    x.floor()
}

#[cfg(feature = "std")]
#[inline]
pub fn roundf(x: f32) -> f32 {
    x.round()
}
