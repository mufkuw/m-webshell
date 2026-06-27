use std::net::IpAddr;
use std::num::NonZeroU32;

use governor::clock::{Clock, DefaultClock};
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter as GovRateLimiter};

pub struct RateLimiter {
    limiter: GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>,
}

impl RateLimiter {
    pub fn new(rate: &str) -> Self {
        let quota = parse_rate(rate).unwrap_or_else(|| {
            // Safe default: 30 per minute.
            Quota::per_minute(NonZeroU32::new(30).unwrap())
        });
        let limiter = GovRateLimiter::direct(quota);
        Self { limiter }
    }

    pub fn check_key(
        &self,
        _ip: &IpAddr,
    ) -> Result<(), governor::NotUntil<<DefaultClock as Clock>::Instant>> {
        self.limiter.check()
    }
}

fn parse_rate(rate: &str) -> Option<Quota> {
    let rate = rate.trim();
    let (count, unit) = rate.split_once('/')?;
    let count = NonZeroU32::new(count.trim().parse::<u32>().ok()?)?;
    let unit = unit.trim();
    Some(match unit {
        "second" | "s" | "sec" => Quota::per_second(count),
        "minute" | "m" | "min" => Quota::per_minute(count),
        "hour" | "h" | "hr" => Quota::per_hour(count),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_common_rates() {
        assert!(parse_rate("30/minute").is_some());
        assert!(parse_rate(" 1 / second ").is_some());
        assert!(parse_rate("0/minute").is_none());
        assert!(parse_rate("abc/minute").is_none());
        assert!(parse_rate("30/fortnight").is_none());
    }
}
