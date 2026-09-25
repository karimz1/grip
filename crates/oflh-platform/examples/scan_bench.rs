//! Developer harness, excluded from cargo build --release --bin oflh.
use oflh_core::{Cancellation, Snapshot, Target};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let target = Target::new(arguments.next().unwrap_or_else(|| ".".into()))?;
    let mode = arguments.next().unwrap_or_else(|| "20".into());
    let mut backend = oflh_platform::native()?;
    if mode == "--dump" {
        dump(&backend.scan(&target, &Cancellation::default())?);
        return Ok(());
    }
    let count: usize = mode.to_str().ok_or("count must be UTF-8")?.parse()?;
    if count == 0 {
        return Err("count must be positive".into());
    }
    let mut times = Vec::with_capacity(count);
    let mut found = 0;
    for _ in 0..count {
        let started = std::time::Instant::now();
        let snapshot = backend.scan(&target, &Cancellation::default())?;
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        found = snapshot.processes.len();
    }
    times.sort_by(f64::total_cmp);
    println!(
        "processes={found} n={count} median_ms={:.3} p95_ms={:.3}",
        times[count / 2],
        times[((count as f64 * 0.95) as usize).min(count - 1)]
    );
    Ok(())
}

/// Emit sorted observations for comparing stable fixtures across implementations.
fn dump(snapshot: &Snapshot) {
    let mut rows = Vec::new();
    for process in &snapshot.processes {
        for usage in &process.usages {
            rows.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                process.identity.pid,
                process.identity.started,
                usage.path.display(),
                usage.relation.label(),
                usage.access.label(),
                usage.deleted,
                usage
                    .lock
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            ));
        }
    }
    rows.sort();
    for row in rows {
        println!("{row}");
    }
}
