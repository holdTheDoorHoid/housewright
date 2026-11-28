//! A small token-bucket rate limiter, per client address and purpose.
//! Used on the endpoints anyone can reach without a device key: pairing
//! invitations, creating a household and recovery lookups.

use std::collections::HashMap;

#[derive(Default)]
pub struct Limiter {
    buckets: HashMap<(&'static str, String), (f64, u64)>,
}

/// Most addresses remembered at once (M4). Past it the least recently
/// seen are dropped; a global bucket per purpose still applies.
const MAX_KEYS: usize = 10_000;

/// The key for an address: IPv6 addresses by their /64 (one home or
/// server usually has a whole /64, so per-address limits would mean
/// nothing), IPv4 as is.
pub fn client_key(ip: &str) -> String {
    match ip.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(v6)) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return v4.to_string();
            }
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
        Ok(std::net::IpAddr::V4(v4)) => v4.to_string(),
        Err(_) => ip.to_string(),
    }
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

/// Everyone together, per purpose: what a flood from many addresses gets.
fn global_rule(bucket: &str) -> (f64, f64) {
    match bucket {
        "pair" => (300.0, 300.0),
        "create" => (20.0, 20.0 / 60.0),
        "recovery" => (100.0, 100.0),
        _ => (600.0, 600.0),
    }
}

fn take(e: &mut (f64, u64), (cap, per_min): (f64, f64), now_ms: u64) -> bool {
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

impl Limiter {
    pub fn allow(&mut self, bucket: &'static str, ip: &str, now_ms: u64) -> bool {
        let g = global_rule(bucket);
        let global = self
            .buckets
            .entry((bucket, "*".to_string()))
            .or_insert((g.0, now_ms));
        if !take(global, g, now_ms) {
            return false;
        }
        let key = client_key(ip);
        if !self.buckets.contains_key(&(bucket, key.clone())) && self.buckets.len() >= MAX_KEYS {
            // Drop the older half (rare; keeps memory bounded).
            let mut times: Vec<u64> = self.buckets.values().map(|v| v.1).collect();
            times.sort_unstable();
            let cut = times[times.len() / 2];
            self.buckets.retain(|k, v| k.1 == "*" || v.1 > cut);
        }
        let r = rule(bucket);
        let e = self.buckets.entry((bucket, key)).or_insert((r.0, now_ms));
        take(e, r, now_ms)
    }

    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
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

    #[test]
    fn ipv6_counts_per_64_and_memory_is_bounded() {
        let mut l = Limiter::default();
        for i in 0..30 {
            assert!(l.allow("pair", &format!("2001:db8:1:2::{i:x}"), 0));
        }
        assert!(!l.allow("pair", "2001:db8:1:2:ffff::1", 0), "same /64");
        for i in 0..30_000u32 {
            let _ = l.allow(
                "other",
                &format!("10.{}.{}.{}", i >> 16, (i >> 8) & 255, i & 255),
                i as u64 * 10_000,
            );
        }
        assert!(l.len() <= MAX_KEYS + 2);
        // The global bucket stops a flood from many addresses.
        let mut l = Limiter::default();
        let ok = (0..1000)
            .filter(|i| l.allow("pair", &format!("10.0.{}.{}", i / 250, i % 250), 0))
            .count();
        assert_eq!(ok, 300);
    }
}
