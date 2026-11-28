//! A small token-bucket rate limiter, per client address and purpose.
//! Used on the endpoints anyone can reach without a device key: pairing
//! invitations, creating a household and recovery lookups.

use std::collections::HashMap;

#[derive(Default)]
pub struct Limiter {
    buckets: HashMap<(&'static str, String), (f64, u64)>,
}

/// (capacity, tokens refilled per minute)
fn rule(bucket: &str) -> (f64, f64) {
    match bucket {
        "pair" => (30.0, 30.0),
        "create" => (5.0, 5.0 / 60.0),
        "recovery" => (10.0, 10.0),
        _ => (60.0, 60.0),
    }
}

impl Limiter {
    pub fn allow(&mut self, bucket: &'static str, ip: &str, now_ms: u64) -> bool {
        if self.buckets.len() > 50_000 {
            self.buckets
                .retain(|_, (_, t)| now_ms.saturating_sub(*t) < 3_600_000);
        }
        let (cap, per_min) = rule(bucket);
        let e = self
            .buckets
            .entry((bucket, ip.to_string()))
            .or_insert((cap, now_ms));
        let elapsed = now_ms.saturating_sub(e.1) as f64 / 60_000.0;
        e.0 = (e.0 + elapsed * per_min).min(cap);
        e.1 = now_ms;
        if e.0 >= 1.0 {
            e.0 -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_drain_and_refill() {
        let mut l = Limiter::default();
        for _ in 0..30 {
            assert!(l.allow("pair", "1.2.3.4", 0));
        }
        assert!(!l.allow("pair", "1.2.3.4", 0));
        assert!(l.allow("pair", "5.6.7.8", 0), "per address");
        assert!(l.allow("pair", "1.2.3.4", 2_000), "refills");
    }
}
