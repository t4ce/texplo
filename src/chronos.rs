#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
pub use std::time::{Duration, SystemTime};
#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
pub use trueos::clock::{Duration, Instant as SystemTime};

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
#[inline]
pub fn elapsed_since(when: SystemTime) -> Duration {
    SystemTime::now()
        .duration_since(when)
        .unwrap_or(Duration::ZERO)
}

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
#[inline]
pub fn elapsed_since(when: SystemTime) -> Duration {
    SystemTime::now().saturating_duration_since(when).into()
}
