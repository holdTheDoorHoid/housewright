//! Hybrid logical clock (Kulkarni et al., 2014).
//!
//! A timestamp is `(wall_ms, counter)`: physical milliseconds plus a counter
//! that breaks ties and keeps the clock moving forward when the physical clock
//! stalls or runs backwards. Every operation gets a fresh, strictly increasing
//! HLC on its device, and receiving remote operations pushes the local clock
//! past them, so "later" in HLC order respects causality.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct Hlc {
    pub wall_ms: u64,
    pub counter: u32,
}

impl Hlc {
    pub const ZERO: Hlc = Hlc {
        wall_ms: 0,
        counter: 0,
    };

    /// The next timestamp for a local event.
    pub fn tick(self, now_ms: u64) -> Hlc {
        if now_ms > self.wall_ms {
            Hlc {
                wall_ms: now_ms,
                counter: 0,
            }
        } else {
            self.bump()
        }
    }

    /// Merge a timestamp received from another device.
    pub fn observe(self, remote: Hlc, now_ms: u64) -> Hlc {
        let wall = self.wall_ms.max(remote.wall_ms).max(now_ms);
        let counter = if wall == self.wall_ms && wall == remote.wall_ms {
            self.counter.max(remote.counter).saturating_add(1)
        } else if wall == self.wall_ms {
            self.counter.saturating_add(1)
        } else if wall == remote.wall_ms {
            remote.counter.saturating_add(1)
        } else {
            0
        };
        Hlc {
            wall_ms: wall,
            counter,
        }
    }

    fn bump(self) -> Hlc {
        match self.counter.checked_add(1) {
            Some(c) => Hlc {
                wall_ms: self.wall_ms,
                counter: c,
            },
            // Counter exhausted: borrow a millisecond.
            None => Hlc {
                wall_ms: self.wall_ms + 1,
                counter: 0,
            },
        }
    }
}

/// Fixed-width hex, so string order equals HLC order (stored in SQLite).
impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}.{:08x}", self.wall_ms, self.counter)
    }
}

impl FromStr for Hlc {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let (w, c) = s.split_once('.').ok_or_else(|| format!("bad HLC '{s}'"))?;
        if w.len() != 16 || c.len() != 8 {
            return Err(format!("bad HLC '{s}'"));
        }
        Ok(Hlc {
            wall_ms: u64::from_str_radix(w, 16).map_err(|e| e.to_string())?,
            counter: u32::from_str_radix(c, 16).map_err(|e| e.to_string())?,
        })
    }
}

impl TryFrom<String> for Hlc {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

impl From<Hlc> for String {
    fn from(h: Hlc) -> String {
        h.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn tick_moves_forward_when_clock_stalls_or_regresses() {
        let a = Hlc::ZERO.tick(100);
        assert_eq!(
            a,
            Hlc {
                wall_ms: 100,
                counter: 0
            }
        );
        let b = a.tick(100);
        assert_eq!(
            b,
            Hlc {
                wall_ms: 100,
                counter: 1
            }
        );
        let c = b.tick(50);
        assert_eq!(
            c,
            Hlc {
                wall_ms: 100,
                counter: 2
            }
        );
        let d = c.tick(200);
        assert_eq!(
            d,
            Hlc {
                wall_ms: 200,
                counter: 0
            }
        );
    }

    #[test]
    fn observe_jumps_past_remote() {
        let local = Hlc {
            wall_ms: 100,
            counter: 5,
        };
        let remote = Hlc {
            wall_ms: 300,
            counter: 7,
        };
        let m = local.observe(remote, 200);
        assert!(m > remote && m > local);
        assert_eq!(
            m,
            Hlc {
                wall_ms: 300,
                counter: 8
            }
        );
        let m = local.observe(
            Hlc {
                wall_ms: 100,
                counter: 9,
            },
            50,
        );
        assert_eq!(
            m,
            Hlc {
                wall_ms: 100,
                counter: 10
            }
        );
        assert_eq!(
            local.observe(remote, 400),
            Hlc {
                wall_ms: 400,
                counter: 0
            }
        );
    }

    #[test]
    fn counter_overflow_borrows_a_millisecond() {
        let h = Hlc {
            wall_ms: 5,
            counter: u32::MAX,
        };
        assert_eq!(
            h.tick(0),
            Hlc {
                wall_ms: 6,
                counter: 0
            }
        );
    }

    proptest! {
        #[test]
        fn string_order_matches_hlc_order(a in any::<(u64, u32)>(), b in any::<(u64, u32)>()) {
            let a = Hlc { wall_ms: a.0, counter: a.1 };
            let b = Hlc { wall_ms: b.0, counter: b.1 };
            prop_assert_eq!(a.cmp(&b), a.to_string().cmp(&b.to_string()));
            prop_assert_eq!(a.to_string().parse::<Hlc>().unwrap(), a);
        }

        #[test]
        fn tick_and_observe_are_monotonic(start in any::<(u32, u16)>(), now in any::<u32>(), r in any::<(u32, u16)>()) {
            let h = Hlc { wall_ms: start.0 as u64, counter: start.1 as u32 };
            let remote = Hlc { wall_ms: r.0 as u64, counter: r.1 as u32 };
            prop_assert!(h.tick(now as u64) > h);
            let o = h.observe(remote, now as u64);
            prop_assert!(o > h && o > remote);
        }
    }
}
