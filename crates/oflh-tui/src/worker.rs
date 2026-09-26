use crate::Event;
use oflh_core::*;
use oflh_platform::Backend;
use std::sync::{Arc, Condvar, Mutex, mpsc::SyncSender};
/// Requests supported by the worker-owned scanner.
pub enum Work {
    Scan(Target),
    ScanPorts(Target),
    Sample(Vec<Identity>),
    Kill(Vec<Identity>, bool),
}
struct Job {
    generation: u64,
    work: Work,
    cancel: Cancellation,
}
#[derive(Default)]
struct Slot {
    job: Option<Job>,
    shutdown: bool,
}
/// One scanner thread with a single replaceable pending request.
pub struct Worker {
    slot: Arc<(Mutex<Slot>, Condvar)>,
    cancel: Cancellation,
}
impl Worker {
    /// Start the scanner thread; native calls run without holding the queue lock.
    pub fn new(mut backend: Box<dyn Backend>, sender: SyncSender<Event>) -> std::io::Result<Self> {
        let slot = Arc::new((Mutex::new(Slot::default()), Condvar::new()));
        let worker = slot.clone();
        std::thread::Builder::new()
            .name("oflh-scanner".into())
            .spawn(move || {
                loop {
                    let (mutex, ready) = &*worker;
                    let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());
                    while state.job.is_none() && !state.shutdown {
                        state = ready.wait(state).unwrap_or_else(|e| e.into_inner())
                    }
                    if state.shutdown {
                        break;
                    }
                    let Some(job) = state.job.take() else {
                        continue;
                    };
                    drop(state);
                    let result = execute_job(&mut *backend, job);
                    if sender.send(result).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            slot,
            cancel: Cancellation::default(),
        })
    }
    /// Cancel previous work and replace any request that has not started yet.
    pub fn request(&mut self, generation: u64, work: Work) {
        self.cancel.cancel();
        self.cancel = Cancellation::default();
        let (mutex, ready) = &*self.slot;
        let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());
        state.job = Some(Job {
            generation,
            work,
            cancel: self.cancel.clone(),
        });
        ready.notify_one();
    }
}
/// Execute one request outside the queue lock so input can cancel or replace work.
fn execute_job(backend: &mut dyn Backend, job: Job) -> Event {
    match job.work {
        Work::Scan(target) => Event::Scan(job.generation, backend.scan(&target, &job.cancel)),
        Work::ScanPorts(target) => Event::Scan(
            job.generation,
            scan_with_ports(backend, &target, &job.cancel),
        ),
        Work::Sample(identities) => {
            Event::Metrics(job.generation, backend.sample(&identities, &job.cancel))
        }
        Work::Kill(identities, force) => {
            let mut sent = 0;
            let mut errors = Vec::new();
            for identity in identities {
                if job.cancel.check().is_err() {
                    errors.push("cancelled remaining actions".into());
                    break;
                }
                match backend.terminate(identity, force, &job.cancel) {
                    Ok(()) => sent += 1,
                    Err(error) => errors.push(format!("PID {}: {error}", identity.pid)),
                }
            }
            Event::Killed(sent, errors)
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.cancel();
        let (mutex, ready) = &*self.slot;
        let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());
        state.shutdown = true;
        state.job = None;
        ready.notify_one();
    }
}

fn scan_with_ports(
    backend: &mut dyn Backend,
    target: &Target,
    cancel: &Cancellation,
) -> Result<Snapshot> {
    let mut snapshot = backend.scan(target, cancel)?;
    match oflh_platform::scan_ports(cancel) {
        Ok(ports) => {
            snapshot.processes.extend(ports.processes);
            snapshot.warnings.extend(ports.warnings);
            snapshot.normalize();
        }
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(error) => snapshot.warnings.push(format!("Port scan failed: {error}")),
    }
    cancel.check()?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, RecvTimeoutError, channel, sync_channel};
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(5);

    struct GatedBackend {
        started: std::sync::mpsc::Sender<()>,
        release: Receiver<()>,
        calls: usize,
    }

    impl Backend for GatedBackend {
        fn scan(&mut self, _: &Target, cancel: &Cancellation) -> Result<Snapshot> {
            self.calls += 1;
            self.started.send(()).unwrap();
            if self.calls == 1 {
                self.release.recv_timeout(TIMEOUT).unwrap();
            }
            cancel.check()?;
            Ok(Snapshot::default())
        }

        fn sample(&mut self, _: &[Identity], _: &Cancellation) -> Result<Vec<(Identity, Metrics)>> {
            panic!("unexpected sampling request")
        }

        fn terminate(&mut self, _: Identity, _: bool, _: &Cancellation) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn pending_refreshes_coalesce_and_cancel_running_scan() {
        let (started_sender, started_receiver) = channel();
        let (release_sender, release_receiver) = channel();
        let (event_sender, event_receiver) = sync_channel(8);
        let backend = GatedBackend {
            started: started_sender,
            release: release_receiver,
            calls: 0,
        };
        let mut worker = Worker::new(Box::new(backend), event_sender).unwrap();
        let target = Target::new(".").unwrap();
        worker.request(1, Work::Scan(target.clone()));
        started_receiver.recv_timeout(TIMEOUT).unwrap();
        worker.request(2, Work::Scan(target.clone()));
        worker.request(3, Work::Scan(target));
        release_sender.send(()).unwrap();
        assert!(matches!(
            event_receiver.recv_timeout(TIMEOUT).unwrap(),
            Event::Scan(1, Err(Error::Cancelled))
        ));
        assert!(matches!(
            event_receiver.recv_timeout(TIMEOUT).unwrap(),
            Event::Scan(3, Ok(_))
        ));
        drop(worker);
        assert!(matches!(
            event_receiver.recv_timeout(TIMEOUT),
            Err(RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn shutdown_discards_pending_actions_without_waiting_for_native_call() {
        let (started_sender, started_receiver) = channel();
        let (release_sender, release_receiver) = channel();
        let (event_sender, event_receiver) = sync_channel(8);
        let backend = GatedBackend {
            started: started_sender,
            release: release_receiver,
            calls: 0,
        };
        let mut worker = Worker::new(Box::new(backend), event_sender).unwrap();
        worker.request(1, Work::Scan(Target::new(".").unwrap()));
        started_receiver.recv_timeout(TIMEOUT).unwrap();
        worker.request(
            2,
            Work::Kill(
                vec![Identity {
                    pid: 42,
                    started: 1,
                    started_sub: 0,
                }],
                true,
            ),
        );
        drop(worker);
        release_sender.send(()).unwrap();
        assert!(matches!(
            event_receiver.recv_timeout(TIMEOUT).unwrap(),
            Event::Scan(1, Err(Error::Cancelled))
        ));
        assert!(matches!(
            event_receiver.recv_timeout(TIMEOUT),
            Err(RecvTimeoutError::Disconnected)
        ));
    }
}
