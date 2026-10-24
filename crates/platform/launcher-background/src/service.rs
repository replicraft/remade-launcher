//! The updater's main loop, independent of how a scan is actually done.
//!
//! The loop owns the schedule: when to scan, when to retry, what to do after
//! the machine sleeps, and what to answer on the socket. What a scan *is*
//! (looking for updates and applying eligible ones) comes from a [`Backend`],
//! so the schedule can be tested with a fake backend and a fake clock.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{Notify, watch};

use crate::ipc::{DaemonInfo, Handler, Request, Response};
use crate::schedule::{self, SavedSchedule};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What one scan did, in the terms the schedule cares about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CycleSummary {
	pub updated: usize,
	pub skipped: usize,
	pub failed: usize,
	/// Every lookup failed, which looks like being offline. Worth a retry.
	pub lookups_failed: bool,
	/// Another process was already scanning, so nothing was done.
	pub busy: bool,
	pub detail: String,
}

pub trait Backend: Send + Sync {
	/// Whether the saved update mode is still the background one.
	fn mode_is_background(&self) -> BoxFuture<'_, bool>;
	fn load_schedule(&self) -> BoxFuture<'_, SavedSchedule>;
	fn save_schedule(&self, schedule: SavedSchedule, outcome: Option<String>) -> BoxFuture<'_, ()>;
	fn scan(&self) -> BoxFuture<'_, CycleSummary>;
}

pub trait Clock: Send + Sync {
	/// Unix seconds, from the wall clock.
	fn now(&self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
	fn now(&self) -> i64 {
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map_or(0, |elapsed| {
				i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
			})
	}
}

pub struct Service {
	backend: Arc<dyn Backend>,
	clock: Arc<dyn Clock>,
	info: Mutex<DaemonInfo>,
	wake: Notify,
	check_requested: AtomicBool,
	stop: watch::Sender<bool>,
}

impl Service {
	pub fn new(
		backend: Arc<dyn Backend>,
		clock: Arc<dyn Clock>,
		pid: u32,
		update_mode: &str,
	) -> Arc<Self> {
		let (stop, _) = watch::channel(false);
		let started_at = clock.now();

		Arc::new(Self {
			backend,
			clock,
			info: Mutex::new(DaemonInfo {
				pid,
				started_at,
				update_mode: update_mode.to_owned(),
				..DaemonInfo::default()
			}),
			wake: Notify::new(),
			check_requested: AtomicBool::new(false),
			stop,
		})
	}

	/// A receiver that changes when the service is asked to stop.
	pub fn stopped(&self) -> watch::Receiver<bool> {
		self.stop.subscribe()
	}

	pub fn request_stop(&self) {
		let _ = self.stop.send(true);
	}

	pub fn snapshot(&self) -> DaemonInfo {
		let mut info = self
			.info
			.lock()
			.map(|info| info.clone())
			.unwrap_or_default();

		info.queue_length = usize::from(self.check_requested.load(Ordering::SeqCst));

		info
	}

	fn update(&self, change: impl FnOnce(&mut DaemonInfo)) {
		if let Ok(mut info) = self.info.lock() {
			change(&mut info);
		}
	}

	/// Runs until stopped, or until the mode is no longer the background one.
	pub async fn run(self: Arc<Self>) {
		let mut stop = self.stop.subscribe();
		let saved = self.backend.load_schedule().await;
		let mut schedule = saved;
		let mut next_due = schedule::first_due(saved, self.clock.now());
		let mut failures = 0u32;

		self.update(|info| {
			info.last_attempted = saved.last_attempted;
			info.last_completed = saved.last_completed;
			info.next_due = Some(next_due);
		});

		loop {
			if !self.backend.mode_is_background().await {
				tracing::info!(
					"the update mode is no longer the background one, so the updater is exiting"
				);

				return;
			}

			let now = self.clock.now();
			let requested = self.check_requested.swap(false, Ordering::SeqCst);

			if schedule::clock_went_backwards(schedule.last_attempted, now) {
				// An unreliable clock must neither shorten a release's age nor
				// cause a repeated scan, so this round is skipped.
				tracing::warn!("the clock moved backwards, so this scan was skipped");

				next_due = schedule::next_after(now);
				schedule.last_attempted = Some(now);
				self.backend
					.save_schedule(schedule, Some("skipped: the clock moved backwards".into()))
					.await;
				self.update(|info| info.next_due = Some(next_due));
			} else if requested || schedule::is_due(next_due, now) {
				self.update(|info| {
					info.cycle_running = true;
					info.last_attempted = Some(now);
				});
				schedule.last_attempted = Some(now);
				self.backend.save_schedule(schedule, None).await;

				let summary = self.backend.scan().await;
				let finished = self.clock.now();

				let retry = if summary.lookups_failed && !summary.busy {
					failures += 1;
					schedule::retry_delay(failures)
				} else {
					failures = 0;
					None
				};

				next_due = match retry {
					Some(delay) => finished + delay,
					None => schedule::next_after(finished),
				};

				// A retry does not count as a completed scan.
				if retry.is_none() && !summary.busy {
					schedule.last_completed = Some(finished);
				}

				let outcome = format!(
					"{} updated, {} skipped, {} failed{}",
					summary.updated,
					summary.skipped,
					summary.failed,
					if summary.busy {
						" (another scan was running)"
					} else {
						""
					}
				);

				schedule.next_due = Some(next_due);
				self.backend
					.save_schedule(schedule, Some(outcome.clone()))
					.await;
				self.update(|info| {
					info.cycle_running = false;
					info.last_completed = schedule.last_completed;
					info.next_due = Some(next_due);
					info.last_outcome = Some(outcome);
				});
			}

			let nap = Duration::from_secs(
				u64::try_from(schedule::nap(next_due, self.clock.now())).unwrap_or(1),
			);

			tokio::select! {
				_ = tokio::time::sleep(nap) => {}
				_ = self.wake.notified() => {}
				_ = stop.changed() => return,
			}
		}
	}
}

impl Handler for Arc<Service> {
	fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + Send + '_>> {
		Box::pin(async move {
			match request {
				Request::Status => Response::Status(self.snapshot()),
				Request::CheckNow => {
					// A second request while one is waiting folds into it.
					let already_queued = self.check_requested.swap(true, Ordering::SeqCst);

					self.wake.notify_one();

					Response::Accepted {
						detail: if already_queued {
							"already queued".into()
						} else {
							"queued".into()
						},
					}
				}
				Request::Stop => {
					self.request_stop();

					Response::Accepted {
						detail: "stopping".into(),
					}
				}
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use std::sync::atomic::{AtomicI64, AtomicUsize};

	use super::*;

	/// A clock the test moves by hand, separate from tokio's paused timer so
	/// a machine sleeping through days can be simulated.
	struct ManualClock(AtomicI64);

	impl ManualClock {
		fn at(seconds: i64) -> Arc<Self> {
			Arc::new(Self(AtomicI64::new(seconds)))
		}

		fn advance(&self, seconds: i64) {
			self.0.fetch_add(seconds, Ordering::SeqCst);
		}
	}

	impl Clock for ManualClock {
		fn now(&self) -> i64 {
			self.0.load(Ordering::SeqCst)
		}
	}

	/// Counts scans and answers from a script.
	struct FakeBackend {
		mode_ok: AtomicBool,
		scans: AtomicUsize,
		saved: Mutex<SavedSchedule>,
		outcomes: Mutex<Vec<CycleSummary>>,
		clock: Arc<ManualClock>,
	}

	impl FakeBackend {
		fn new(clock: Arc<ManualClock>) -> Arc<Self> {
			Arc::new(Self {
				mode_ok: AtomicBool::new(true),
				scans: AtomicUsize::new(0),
				saved: Mutex::new(SavedSchedule::default()),
				outcomes: Mutex::new(Vec::new()),
				clock,
			})
		}

		fn scans(&self) -> usize {
			self.scans.load(Ordering::SeqCst)
		}
	}

	impl Backend for FakeBackend {
		fn mode_is_background(&self) -> BoxFuture<'_, bool> {
			Box::pin(async move { self.mode_ok.load(Ordering::SeqCst) })
		}

		fn load_schedule(&self) -> BoxFuture<'_, SavedSchedule> {
			Box::pin(async move { *self.saved.lock().unwrap() })
		}

		fn save_schedule(&self, schedule: SavedSchedule, _: Option<String>) -> BoxFuture<'_, ()> {
			Box::pin(async move { *self.saved.lock().unwrap() = schedule })
		}

		fn scan(&self) -> BoxFuture<'_, CycleSummary> {
			Box::pin(async move {
				self.scans.fetch_add(1, Ordering::SeqCst);

				let mut script = self.outcomes.lock().unwrap();

				if script.is_empty() {
					CycleSummary::default()
				} else {
					script.remove(0)
				}
			})
		}
	}

	const START: i64 = 1_900_000_000;

	/// Lets the service run until it is waiting again, with the clock and
	/// tokio's timer moved together by `seconds`.
	async fn pass(clock: &ManualClock, seconds: i64) {
		let step = 300;
		let mut moved = 0;

		while moved < seconds {
			let by = step.min(seconds - moved);

			clock.advance(by);
			tokio::time::advance(Duration::from_secs(u64::try_from(by).unwrap())).await;
			tokio::task::yield_now().await;
			moved += by;
		}
	}

	fn start(backend: &Arc<FakeBackend>, clock: &Arc<ManualClock>) -> Arc<Service> {
		let service = Service::new(backend.clone(), clock.clone(), 1, "daemon");

		tokio::spawn(Arc::clone(&service).run());

		service
	}

	#[tokio::test(start_paused = true)]
	async fn it_scans_at_start_and_then_once_a_day() {
		let clock = ManualClock::at(START);
		let backend = FakeBackend::new(clock.clone());
		let _service = start(&backend, &clock);

		tokio::task::yield_now().await;
		assert_eq!(backend.scans(), 1, "one scan at activation");

		pass(&clock, 23 * 3_600).await;
		assert_eq!(backend.scans(), 1, "nothing for almost a day");

		pass(&clock, 2 * 3_600).await;
		assert_eq!(backend.scans(), 2, "then the daily scan");
	}

	#[tokio::test(start_paused = true)]
	async fn a_restart_just_after_a_scan_does_not_scan_again() {
		let clock = ManualClock::at(START);
		let backend = FakeBackend::new(clock.clone());
		*backend.saved.lock().unwrap() = SavedSchedule {
			last_attempted: Some(START - 120),
			last_completed: Some(START - 100),
			next_due: None,
		};

		let _service = start(&backend, &clock);

		tokio::task::yield_now().await;
		assert_eq!(backend.scans(), 0);
	}

	#[tokio::test(start_paused = true)]
	async fn sleeping_through_days_yields_exactly_one_catch_up_scan() {
		let clock = ManualClock::at(START);
		let backend = FakeBackend::new(clock.clone());
		let _service = start(&backend, &clock);

		tokio::task::yield_now().await;
		assert_eq!(backend.scans(), 1);

		// The machine sleeps for four days: the wall clock jumps, the timer barely moves.
		clock.advance(4 * 86_400);
		tokio::time::advance(Duration::from_secs(15 * 60)).await;
		tokio::task::yield_now().await;

		assert_eq!(backend.scans(), 2, "one catch-up, not four scans");

		pass(&clock, 3_600).await;
		assert_eq!(backend.scans(), 2);
	}

	#[tokio::test(start_paused = true)]
	async fn a_failed_scan_retries_with_backoff_and_then_waits_the_normal_day() {
		let clock = ManualClock::at(START);
		let backend = FakeBackend::new(clock.clone());
		let offline = CycleSummary {
			lookups_failed: true,
			failed: 1,
			..CycleSummary::default()
		};
		*backend.outcomes.lock().unwrap() =
			vec![offline.clone(), offline.clone(), offline.clone(), offline];
		let _service = start(&backend, &clock);

		tokio::task::yield_now().await;
		assert_eq!(backend.scans(), 1);

		pass(&clock, 5 * 60 + 5).await;
		assert_eq!(backend.scans(), 2, "first retry after five minutes");

		pass(&clock, 10 * 60 + 5).await;
		assert_eq!(backend.scans(), 3, "second after ten");

		pass(&clock, 20 * 60 + 5).await;
		assert_eq!(backend.scans(), 4, "third after twenty");

		pass(&clock, 3 * 3_600).await;
		assert_eq!(
			backend.scans(),
			4,
			"retries are used up, so it waits for the daily scan"
		);

		pass(&clock, 24 * 3_600).await;
		assert_eq!(backend.scans(), 5);
	}

	#[tokio::test(start_paused = true)]
	async fn check_now_scans_immediately_and_two_requests_become_one() {
		let clock = ManualClock::at(START);
		let backend = FakeBackend::new(clock.clone());
		let service = start(&backend, &clock);

		tokio::task::yield_now().await;
		assert_eq!(backend.scans(), 1);

		let first = service.handle(Request::CheckNow).await;
		let second = service.handle(Request::CheckNow).await;

		assert_eq!(
			first,
			Response::Accepted {
				detail: "queued".into()
			}
		);
		assert_eq!(
			second,
			Response::Accepted {
				detail: "already queued".into()
			}
		);
		assert_eq!(service.snapshot().queue_length, 1);

		tokio::task::yield_now().await;
		tokio::task::yield_now().await;

		assert_eq!(backend.scans(), 2, "both requests were served by one scan");
		assert_eq!(service.snapshot().queue_length, 0);
	}

	#[tokio::test(start_paused = true)]
	async fn the_updater_exits_when_the_mode_is_no_longer_the_background_one() {
		let clock = ManualClock::at(START);
		let backend = FakeBackend::new(clock.clone());
		let service = Service::new(backend.clone(), clock.clone(), 1, "daemon");
		let handle = tokio::spawn(Arc::clone(&service).run());

		tokio::task::yield_now().await;
		backend.mode_ok.store(false, Ordering::SeqCst);
		pass(&clock, 20 * 60).await;

		assert!(handle.is_finished());
		assert_eq!(backend.scans(), 1);
	}

	#[tokio::test(start_paused = true)]
	async fn a_clock_that_goes_backwards_skips_the_round() {
		let clock = ManualClock::at(START);
		let backend = FakeBackend::new(clock.clone());
		let _service = start(&backend, &clock);

		tokio::task::yield_now().await;
		assert_eq!(backend.scans(), 1);

		clock.advance(-3 * 3_600);
		pass(&clock, 20 * 60).await;
		assert_eq!(backend.scans(), 1, "no scan on an untrustworthy clock");
	}

	#[tokio::test(start_paused = true)]
	async fn a_stop_request_ends_the_loop_and_status_tells_the_truth() {
		let clock = ManualClock::at(START);
		let backend = FakeBackend::new(clock.clone());
		backend.outcomes.lock().unwrap().push(CycleSummary {
			updated: 2,
			skipped: 1,
			..CycleSummary::default()
		});
		let service = Service::new(backend.clone(), clock.clone(), 42, "daemon");
		let handle = tokio::spawn(Arc::clone(&service).run());

		tokio::task::yield_now().await;

		let Response::Status(info) = service.handle(Request::Status).await else {
			panic!("expected status")
		};
		assert_eq!(info.pid, 42);
		assert_eq!(
			info.last_outcome.as_deref(),
			Some("2 updated, 1 skipped, 0 failed")
		);
		assert_eq!(info.next_due, Some(START + 86_400));
		assert!(!info.cycle_running);

		assert_eq!(
			service.handle(Request::Stop).await,
			Response::Accepted {
				detail: "stopping".into()
			}
		);
		tokio::task::yield_now().await;

		assert!(handle.is_finished());
	}
}
