//! Startup allowance for CPU images; file size is not an allocation estimate.
use std::sync::OnceLock;
const MIB: u64 = 1024 * 1024;

fn allowance(total: u64, available: u64) -> u64 {
    // Leave half of physical memory, and a quarter of current headroom, outside
    // Fold's image allowance. Other application caches retain their own limits.
    (total / 2)
        .min(available.saturating_mul(3) / 4)
        .clamp(128 * MIB, 16 * 1024 * MIB)
}

pub fn automatic() -> u64 {
    static BYTES: OnceLock<u64> = OnceLock::new();
    *BYTES.get_or_init(|| {
        #[cfg(target_os = "linux")]
        if let Ok(info) = std::fs::read_to_string("/proc/meminfo") {
            let field = |key: &str| {
                info.lines().find_map(|line| {
                    let mut fields = line.split_whitespace();
                    (fields.next()? == key)
                        .then(|| fields.next()?.parse::<u64>().ok()?.checked_mul(1024))
                        .flatten()
                })
            };
            if let (Some(mut total), Some(mut available)) =
                (field("MemTotal:"), field("MemAvailable:"))
            {
                // Respect a cgroup-v2 memory ceiling when present.
                if let Ok(limit) = std::fs::read_to_string("/sys/fs/cgroup/memory.max")
                    && let Ok(limit) = limit.trim().parse::<u64>()
                {
                    total = total.min(limit);
                    if let Ok(used) = std::fs::read_to_string("/sys/fs/cgroup/memory.current")
                        && let Ok(used) = used.trim().parse::<u64>()
                    {
                        available = available.min(limit.saturating_sub(used));
                    }
                }
                return allowance(total, available);
            }
        }
        // Platforms without this telemetry retain an explicit portable default;
        // manual Application memory preferences remain authoritative.
        768 * MIB
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_allowance_leaves_headroom_and_obeys_ceiling() {
        assert_eq!(allowance(32 * 1024 * MIB, 24 * 1024 * MIB), 16 * 1024 * MIB);
        assert_eq!(allowance(8 * 1024 * MIB, 4 * 1024 * MIB), 3 * 1024 * MIB);
        assert_eq!(
            allowance(128 * 1024 * MIB, 64 * 1024 * MIB),
            16 * 1024 * MIB
        );
        assert_eq!(allowance(1024 * MIB, 128 * MIB), 128 * MIB);
    }
}
