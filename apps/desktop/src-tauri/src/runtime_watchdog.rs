use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex, TryLockError, Weak,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::launcher::{RuntimeManager, RUNTIME_HEALTH_PROBE_TIMEOUT};

pub(crate) const RUNTIME_WATCHDOG_INTERVAL: Duration = Duration::from_secs(30);
const RUNTIME_LOCK_TIMEOUT: Duration = Duration::from_millis(250);
const RUNTIME_LOCK_POLL: Duration = Duration::from_millis(10);

trait WatchdogTarget: Send + Sync {
    fn tick(&self, deadline: Instant, cancelled: &AtomicBool);
}

impl WatchdogTarget for Mutex<RuntimeManager> {
    fn tick(&self, lock_deadline: Instant, cancelled: &AtomicBool) {
        let mut runtime = loop {
            if cancelled.load(Ordering::Acquire) || Instant::now() >= lock_deadline {
                return;
            }
            match self.try_lock() {
                Ok(runtime) => break runtime,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) => {
                    thread::park_timeout(
                        lock_deadline
                            .saturating_duration_since(Instant::now())
                            .min(RUNTIME_LOCK_POLL),
                    );
                }
            }
        };
        // Lock contention skips this period instead of consuming the health
        // budget and falsely failing an otherwise healthy proxy. Once the
        // exact runtime is reserved, its network checks get one fresh total
        // deadline shared by every endpoint and Controller operation.
        let _ = runtime
            .activation_for_watchdog(Instant::now() + RUNTIME_HEALTH_PROBE_TIMEOUT, cancelled);
    }
}

enum WatchdogSignal {
    Shutdown,
    #[cfg(test)]
    Tick {
        started: Sender<()>,
        completed: Sender<()>,
    },
}

/// Owns the one native scheduler that refreshes runtime health independently
/// of WebView timers. The worker retains only a weak reference to the runtime,
/// so it cannot extend AppState lifetime or keep a retired runtime reachable.
pub(crate) struct RuntimeWatchdog {
    signal: Sender<WatchdogSignal>,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl RuntimeWatchdog {
    pub(crate) fn start(runtime: &Arc<Mutex<RuntimeManager>>) -> io::Result<Self> {
        Self::start_with_interval(runtime, RUNTIME_WATCHDOG_INTERVAL)
    }

    fn start_with_interval(
        runtime: &Arc<Mutex<RuntimeManager>>,
        interval: Duration,
    ) -> io::Result<Self> {
        let target: Arc<dyn WatchdogTarget> = runtime.clone();
        Self::start_target(Arc::downgrade(&target), interval)
    }

    fn start_target(target: Weak<dyn WatchdogTarget>, interval: Duration) -> io::Result<Self> {
        let (signal, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("verisilo-runtime-watchdog".to_owned())
            .spawn(move || run_watchdog(target, receiver, interval, worker_cancelled))?;
        Ok(Self {
            signal,
            cancelled,
            worker: Some(worker),
        })
    }

    pub(crate) fn shutdown(&mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };
        self.cancelled.store(true, Ordering::Release);
        let _ = self.signal.send(WatchdogSignal::Shutdown);
        let _ = worker.join();
    }

    #[cfg(test)]
    pub(crate) fn tick_and_wait(&self) {
        let (started_tx, started) = mpsc::channel();
        let (completed_tx, completed) = mpsc::channel();
        self.signal
            .send(WatchdogSignal::Tick {
                started: started_tx,
                completed: completed_tx,
            })
            .expect("runtime watchdog accepts deterministic tick");
        started
            .recv_timeout(Duration::from_secs(5))
            .expect("runtime watchdog starts deterministic tick");
        completed
            .recv_timeout(RUNTIME_HEALTH_PROBE_TIMEOUT + Duration::from_secs(1))
            .expect("runtime watchdog acknowledges deterministic tick");
    }

    #[cfg(test)]
    fn trigger_tick(&self) -> (Receiver<()>, Receiver<()>) {
        let (started_tx, started) = mpsc::channel();
        let (completed_tx, completed) = mpsc::channel();
        self.signal
            .send(WatchdogSignal::Tick {
                started: started_tx,
                completed: completed_tx,
            })
            .expect("runtime watchdog accepts deterministic tick");
        (started, completed)
    }
}

impl Drop for RuntimeWatchdog {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_watchdog(
    target: Weak<dyn WatchdogTarget>,
    receiver: Receiver<WatchdogSignal>,
    interval: Duration,
    cancelled: Arc<AtomicBool>,
) {
    loop {
        let acknowledgements: Option<(Sender<()>, Sender<()>)> =
            match receiver.recv_timeout(interval) {
                Ok(WatchdogSignal::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => None,
                #[cfg(test)]
                Ok(WatchdogSignal::Tick { started, completed }) => Some((started, completed)),
            };

        let Some(target) = target.upgrade() else {
            if let Some((started, completed)) = acknowledgements {
                let _ = started.send(());
                let _ = completed.send(());
            }
            break;
        };
        if let Some((started, _)) = acknowledgements.as_ref() {
            let _ = started.send(());
        }
        target.tick(Instant::now() + RUNTIME_LOCK_TIMEOUT, &cancelled);
        if let Some((_, completed)) = acknowledgements {
            let _ = completed.send(());
        }
        if cancelled.load(Ordering::Acquire) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Default)]
    struct CountingTarget {
        ticks: AtomicUsize,
    }

    impl WatchdogTarget for CountingTarget {
        fn tick(&self, _deadline: Instant, _cancelled: &AtomicBool) {
            self.ticks.fetch_add(1, Ordering::AcqRel);
        }
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn watchdog_can_be_owned_by_tauri_managed_state() {
        assert_send_sync::<RuntimeWatchdog>();
    }

    #[test]
    fn deterministic_tick_runs_without_waiting_for_the_production_interval() {
        assert_eq!(RUNTIME_WATCHDOG_INTERVAL, Duration::from_secs(30));
        let target = Arc::new(CountingTarget::default());
        let erased: Arc<dyn WatchdogTarget> = target.clone();
        let mut watchdog =
            RuntimeWatchdog::start_target(Arc::downgrade(&erased), RUNTIME_WATCHDOG_INTERVAL)
                .expect("start runtime watchdog");

        watchdog.tick_and_wait();

        assert_eq!(target.ticks.load(Ordering::Acquire), 1);
        watchdog.shutdown();
        assert!(watchdog.worker.is_none());
    }

    #[test]
    fn worker_keeps_only_a_weak_target_and_drop_joins_it() {
        let target = Arc::new(CountingTarget::default());
        let weak_target = Arc::downgrade(&target);
        let erased: Arc<dyn WatchdogTarget> = target.clone();
        let watchdog =
            RuntimeWatchdog::start_target(Arc::downgrade(&erased), RUNTIME_WATCHDOG_INTERVAL)
                .expect("start runtime watchdog");
        drop(erased);
        drop(target);

        assert!(weak_target.upgrade().is_none());
        drop(watchdog);
    }

    #[test]
    fn shutdown_cancels_a_tick_waiting_for_the_runtime_mutex() {
        let runtime = Arc::new(Mutex::new(RuntimeManager::default()));
        let runtime_guard = runtime.lock().expect("hold runtime mutex");
        let mut watchdog =
            RuntimeWatchdog::start_with_interval(&runtime, RUNTIME_WATCHDOG_INTERVAL)
                .expect("start runtime watchdog");
        let (started, completed) = watchdog.trigger_tick();
        started
            .recv_timeout(Duration::from_secs(1))
            .expect("watchdog begins contended tick");

        let (shutdown_done_tx, shutdown_done) = mpsc::channel();
        let shutdown_thread = thread::spawn(move || {
            watchdog.shutdown();
            let _ = shutdown_done_tx.send(());
            watchdog
        });
        let shutdown_result = shutdown_done.recv_timeout(Duration::from_secs(1));
        if shutdown_result.is_err() {
            drop(runtime_guard);
            let _ = shutdown_thread.join();
            panic!("watchdog shutdown waited for the contended runtime mutex");
        }

        assert!(completed.recv_timeout(Duration::from_secs(1)).is_ok());
        drop(runtime_guard);
        let watchdog = shutdown_thread.join().expect("shutdown thread exits");
        assert!(watchdog.worker.is_none());
    }
}
