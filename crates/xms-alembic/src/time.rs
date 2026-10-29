//! Time samplings: when each sample of a property is, in seconds.
//!
//! - **Uniform:** one time, then a sample every `time_per_cycle` seconds.
//! - **Cyclic:** several times in a cycle, the cycle repeated every
//!   `time_per_cycle` seconds.
//! - **Acyclic:** every sample's time listed.
//!
//! The archive lists its time samplings once; each property names one by its
//! index. Index 0 is the default: uniform, from 0, a second apart.

/// The time per cycle Alembic writes for an acyclic sampling.
const ACYCLIC_TIME_PER_CYCLE: f64 = f64::MAX / 32.0;

/// How a property's samples are placed in time.
#[derive(Clone, Debug, PartialEq)]
pub enum TimeSampling {
    Uniform { start: f64, time_per_cycle: f64 },
    Cyclic { times: Vec<f64>, time_per_cycle: f64 },
    Acyclic { times: Vec<f64> },
}

impl Default for TimeSampling {
    fn default() -> Self {
        TimeSampling::Uniform {
            start: 0.0,
            time_per_cycle: 1.0,
        }
    }
}

impl TimeSampling {
    /// As written: its times and time per cycle.
    pub(crate) fn from_parts(times: Vec<f64>, time_per_cycle: f64) -> TimeSampling {
        if time_per_cycle == ACYCLIC_TIME_PER_CYCLE || time_per_cycle <= 0.0 {
            TimeSampling::Acyclic { times }
        } else if times.len() == 1 {
            TimeSampling::Uniform {
                start: times[0],
                time_per_cycle,
            }
        } else {
            TimeSampling::Cyclic { times, time_per_cycle }
        }
    }

    /// When sample `i` is, in seconds.
    pub fn time(&self, i: usize) -> f64 {
        match self {
            TimeSampling::Uniform { start, time_per_cycle } => start + time_per_cycle * i as f64,
            TimeSampling::Cyclic { times, time_per_cycle } => {
                let n = times.len().max(1);
                times.get(i % n).copied().unwrap_or(0.0) + time_per_cycle * (i / n) as f64
            }
            TimeSampling::Acyclic { times } => times.get(i).or(times.last()).copied().unwrap_or(0.0),
        }
    }
}

/// The time samplings of an archive, each with the most samples a property
/// using it has.
pub(crate) fn read_time_samplings(bytes: &[u8]) -> Result<Vec<(TimeSampling, u32)>, crate::Error> {
    let mut out = vec![];
    let mut pos = 0;
    let bad = crate::Error::Corrupt("the archive's time samplings");
    while pos < bytes.len() {
        if pos + 16 > bytes.len() {
            return Err(bad);
        }
        let max = u32::from_le_bytes(bytes[pos..pos + 4].try_into().expect("4 bytes"));
        let tpc = f64::from_le_bytes(bytes[pos + 4..pos + 12].try_into().expect("8 bytes"));
        let n = u32::from_le_bytes(bytes[pos + 12..pos + 16].try_into().expect("4 bytes")) as usize;
        pos += 16;
        if n == 0 || pos + n * 8 > bytes.len() {
            return Err(bad);
        }
        let times = bytes[pos..pos + n * 8]
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().expect("8 bytes")))
            .collect();
        pos += n * 8;
        out.push((TimeSampling::from_parts(times, tpc), max));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_placed_in_time() {
        let u = TimeSampling::from_parts(vec![1.0 / 24.0], 1.0 / 24.0);
        assert!((u.time(23) - 1.0).abs() < 1e-12);
        let c = TimeSampling::from_parts(vec![0.0, 0.25], 1.0);
        assert_eq!((c.time(0), c.time(1), c.time(2), c.time(3)), (0.0, 0.25, 1.0, 1.25));
        let a = TimeSampling::from_parts(vec![0.0, 0.1, 0.5], ACYCLIC_TIME_PER_CYCLE);
        assert_eq!((a.time(2), a.time(9)), (0.5, 0.5));
    }
}
