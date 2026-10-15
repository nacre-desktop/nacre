//! Restart one failed service; never restart unrelated tasks or log panic payloads.
use super::hub::Shared;
use crate::Result;
use std::{
    collections::VecDeque,
    future::Future,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Failures {
    recent: VecDeque<Instant>,
    backoff: u32,
}
impl Failures {
    pub fn record(&mut self, now: Instant) -> Option<Duration> {
        while self
            .recent
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= Duration::from_secs(120))
        {
            self.recent.pop_front();
        }
        self.recent.push_back(now);
        if self.recent.len() >= 5 {
            return None;
        }
        let delay =
            Duration::from_millis((250u64.saturating_mul(1u64 << self.backoff.min(7))).min(30_000));
        self.backoff = self.backoff.saturating_add(1);
        Some(delay)
    }
    pub fn count(&self) -> usize {
        self.recent.len()
    }
}
pub async fn run<F, Fut>(name: &'static str, hub: Shared, make: F)
where
    F: Fn() -> Fut + Clone + 'static,
    Fut: Future<Output = Result<()>> + 'static,
{
    if hub.borrow().safe && !matches!(name, "compositor" | "settings" | "notifications") {
        if let Err(error) = hub.borrow_mut().service(name, "safe-mode", 0) {
            tracing::error!(service=name,code=?error.code,"Cannot publish safe-mode status");
        }
        return;
    }
    let mut failures = Failures::default();
    loop {
        if let Err(error) = hub.borrow_mut().service(name, "starting", failures.count()) {
            tracing::error!(service=name,code=?error.code,"Cannot publish service status");
        }
        let make = make.clone();
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn_local(async move { make().await });
        match tasks.join_next().await {
            Some(Ok(Ok(()))) => tracing::warn!(service = name, "Persistent service ended"),
            Some(Ok(Err(_))) => {
                tracing::warn!(service = name, "Service connection or operation failed")
            }
            _ => tracing::error!(service = name, "Service task failed"),
        }
        let delay = failures.record(Instant::now());
        if let Err(error) = hub.borrow_mut().service(
            name,
            if delay.is_some() {
                "restarting"
            } else {
                "disabled"
            },
            failures.count(),
        ) {
            tracing::error!(service=name,code=?error.code,"Cannot publish failure status");
        }
        let Some(delay) = delay else {
            return;
        };
        tokio::time::sleep(delay).await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_budget_and_backoff_are_bounded() {
        let now = Instant::now();
        let mut failures = Failures::default();
        for (i, ms) in [250, 500, 1000, 2000].into_iter().enumerate() {
            assert_eq!(
                failures.record(now + Duration::from_secs(i as u64)),
                Some(Duration::from_millis(ms))
            );
        }
        assert_eq!(failures.record(now + Duration::from_secs(4)), None);
        assert!(failures.record(now + Duration::from_secs(125)).is_some());
        for n in 2..15 {
            assert!(
                failures.record(now + Duration::from_secs(125 * n)).unwrap()
                    <= Duration::from_secs(30)
            );
        }
    }
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn panicking_service_stops_after_five_failures_while_another_task_stays_up() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let hub =
                    super::super::hub::Hub::new("fixture".into(), false, Duration::from_millis(16))
                        .shared();
                let state = hub.clone();
                let healthy = tokio::task::spawn_local(run("healthy", hub.clone(), move || {
                    let state = state.clone();
                    async move {
                        state.borrow_mut().service("healthy", "running", 0)?;
                        std::future::pending::<()>().await;
                        Ok(())
                    }
                }));
                async fn panic_task() -> Result<()> {
                    panic!("synthetic service failure")
                }
                run("broken", hub.clone(), panic_task).await;
                assert_eq!(hub.borrow().services["broken"]["status"], "disabled");
                assert_eq!(hub.borrow().services["broken"]["failures"], 5);
                assert_eq!(hub.borrow().services["healthy"]["status"], "running");
                healthy.abort();
            })
            .await;
    }
}
