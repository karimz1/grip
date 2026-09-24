use crate::Event;
use oflh_core::*;
use oflh_platform::Backend;
use std::sync::{Arc, Condvar, Mutex, mpsc::SyncSender};
pub enum Work {
    Scan(Target),
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
pub struct Worker {
    slot: Arc<(Mutex<Slot>, Condvar)>,
    cancel: Cancellation,
}
impl Worker {
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
                    let result = match job.work {
                        Work::Scan(target) => {
                            Event::Scan(job.generation, backend.scan(&target, &job.cancel))
                        }
                        Work::Sample(ids) => {
                            Event::Metrics(job.generation, backend.sample(&ids, &job.cancel))
                        }
                        Work::Kill(ids, force) => {
                            let mut sent = 0;
                            let mut errors = Vec::new();
                            for id in ids {
                                if job.cancel.check().is_err() {
                                    errors.push("cancelled remaining actions".into());
                                    break;
                                }
                                match backend.terminate(id, force, &job.cancel) {
                                    Ok(()) => sent += 1,
                                    Err(e) => errors.push(format!("PID {}: {e}", id.pid)),
                                }
                            }
                            Event::Killed(sent, errors)
                        }
                    };
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
