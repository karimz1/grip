//! Native OS boundaries. A scanner is owned by one worker, never shared concurrently.
use oflh_core::*;
use std::collections::HashMap;
#[cfg(not(target_os = "linux"))]
use std::time::Instant;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

/// Operations provided by a single worker-owned native scanner.
pub trait Backend: Send + 'static {
    /// Collect current observations, honoring cancellation between native calls.
    fn scan(&mut self, target: &Target, cancel: &Cancellation) -> Result<Snapshot>;
    /// Sample resources only for the supplied process birth identities.
    fn sample(
        &mut self,
        ids: &[Identity],
        cancel: &Cancellation,
    ) -> Result<Vec<(Identity, Metrics)>>;
    /// Request termination after validating identity and protected-process guards.
    fn terminate(&mut self, id: Identity, force: bool, cancel: &Cancellation) -> Result<()>;
}
/// Construct the scanner for the current operating system.
pub fn native() -> Result<Box<dyn Backend>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::Native::default()))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::Native::default()))
    }
    #[cfg(windows)]
    {
        Ok(Box::new(windows::Native::default()))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        Err(Error::Unavailable("unsupported operating system".into()))
    }
}
#[derive(Default)]
struct Sampler {
    previous: HashMap<Identity, (u64, u64)>,
    #[cfg(not(target_os = "linux"))]
    epoch: Option<Instant>,
}
impl Sampler {
    #[cfg(not(target_os = "linux"))]
    fn clock(&mut self) -> u64 {
        let epoch = self.epoch.get_or_insert_with(Instant::now);
        epoch.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }
    fn sample(
        &mut self,
        raw: Vec<(Identity, u64, Option<u64>)>,
        total: u64,
    ) -> Vec<(Identity, Metrics)> {
        let mut next = HashMap::with_capacity(raw.len());
        let result = raw
            .into_iter()
            .map(|(id, cpu, memory)| {
                let percent = self.previous.get(&id).and_then(|&(prev, old)| {
                    (total > old && cpu >= prev)
                        .then(|| (100.0 * (cpu - prev) as f64 / (total - old) as f64).min(100.0))
                });
                next.insert(id, (cpu, total));
                (
                    id,
                    Metrics {
                        cpu: percent,
                        memory,
                    },
                )
            })
            .collect();
        self.previous = next;
        result
    }
}
fn apply_metrics(snapshot: &mut Snapshot, metrics: Vec<(Identity, Metrics)>) {
    let metrics: HashMap<_, _> = metrics.into_iter().collect();
    for p in &mut snapshot.processes {
        if let Some(m) = metrics.get(&p.identity) {
            p.memory = m.memory;
            p.cpu = m.cpu;
        }
    }
}
