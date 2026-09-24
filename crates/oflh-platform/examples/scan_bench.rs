//! Developer harness, excluded from cargo build --release --bin oflh.
use oflh_core::{Cancellation, Target};
fn main() {
    let mut args = std::env::args_os().skip(1);
    let target = Target::new(args.next().unwrap_or_else(|| ".".into())).unwrap();
    let count = args
        .next()
        .and_then(|s| s.to_str().and_then(|s| s.parse::<usize>().ok()))
        .unwrap_or(20);
    let mut backend = oflh_platform::native().unwrap();
    let mut times = Vec::new();
    let mut found = 0;
    for _ in 0..count {
        let now = std::time::Instant::now();
        let r = backend.scan(&target, &Cancellation::default()).unwrap();
        times.push(now.elapsed().as_secs_f64() * 1000.0);
        found = r.processes.len();
    }
    times.sort_by(f64::total_cmp);
    println!(
        "processes={found} n={count} median_ms={:.3} p95_ms={:.3}",
        times[count / 2],
        times[((count as f64 * 0.95) as usize).min(count - 1)]
    );
}
