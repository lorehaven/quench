use super::Runner;
use crate::bridge::{Call, Plugin, string_arg};
use quench_serde::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Once, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A reminder that waits in this process.
#[derive(Debug, Clone)]
struct Reminder {
    at_ms: i64,
    title: String,
    body: String,
    channel: String,
}

/// Desktop notifications through `notify-send` (the freedesktop notification service). `show` posts one now.
///
/// `schedule` posts one at a time while the app is running. There is no process to wake the app when it is
/// closed, so a reminder does not outlive it, and repeating reminders (`repeat_zone`/`repeat_time`) answer
/// `unsupported`.
pub struct Notifications {
    app: String,
    run: Arc<dyn Runner>,
    waiting: Arc<Waiting>,
    /// Starts the one thread that posts the reminders, when the first one is scheduled.
    started: Once,
}

/// The reminders that wait, and what wakes the thread that posts them when they change.
#[derive(Default)]
struct Waiting {
    reminders: Mutex<BTreeMap<i64, Reminder>>,
    changed: Condvar,
}

impl std::fmt::Debug for Notifications {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notifications")
            .field("app", &self.app)
            .finish_non_exhaustive()
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

impl Notifications {
    /// Notifications from the app with this name.
    #[must_use]
    pub fn new(app: &str, run: Arc<dyn Runner>) -> Self {
        Self {
            app: app.to_string(),
            run,
            waiting: Arc::new(Waiting::default()),
            started: Once::new(),
        }
    }

    fn post(&self, id: i64, title: &str, body: &str) -> bool {
        post(self.run.as_ref(), &self.app, id, title, body)
    }

    fn waiting(&self) -> MutexGuard<'_, BTreeMap<i64, Reminder>> {
        self.waiting
            .reminders
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn schedule(&self, id: i64, reminder: &Reminder) {
        self.waiting().insert(id, reminder.clone());
        self.started.call_once(|| {
            let (run, app, waiting) = (
                Arc::clone(&self.run),
                self.app.clone(),
                Arc::clone(&self.waiting),
            );
            std::thread::spawn(move || post_when_due(run.as_ref(), &app, &waiting));
        });
        self.waiting.changed.notify_all();
    }

    fn cancel(&self, id: i64) {
        self.waiting().remove(&id);
        self.waiting.changed.notify_all();
    }
}

/// The one thread that posts reminders: it sleeps until the earliest is due (or until the set changes),
/// posts what is due and goes on. It lives as long as the process; with nothing waiting it sleeps for good.
fn post_when_due(run: &dyn Runner, app: &str, waiting: &Waiting) {
    let mut reminders = waiting
        .reminders
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    loop {
        let now = now_ms();
        let due: Vec<i64> = reminders
            .iter()
            .filter(|(_, r)| r.at_ms <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            if let Some(r) = reminders.remove(&id) {
                // Posting runs a program: not while holding the lock that `schedule` and `cancel` need.
                drop(reminders);
                post(run, app, id, &r.title, &r.body);
                reminders = waiting
                    .reminders
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }
        let next = reminders.values().map(|r| r.at_ms).min();
        reminders = match next {
            None => waiting
                .changed
                .wait(reminders)
                .unwrap_or_else(PoisonError::into_inner),
            Some(at) => {
                let wait = Duration::from_millis(u64::try_from(at - now_ms()).unwrap_or(0).max(1));
                waiting
                    .changed
                    .wait_timeout(reminders, wait)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
        };
    }
}

fn post(run: &dyn Runner, app: &str, id: i64, title: &str, body: &str) -> bool {
    let id = id.to_string();
    run.run(
        "notify-send",
        &["--app-name", app, "--replace-id", &id, title, body],
        None,
    )
    .is_ok_and(|out| out.success)
}

impl Plugin for Notifications {
    fn name(&self) -> &'static str {
        "notifications"
    }

    fn call(&self, method: &str, args: &Value, call: Call) {
        let id = args.get("id").and_then(Value::as_i64).unwrap_or(0);
        match method {
            // Channels are an Android notion.
            "channel" => call.resolve(&Value::Null),
            "permission" => call.resolve(&json!({ "granted": true })),
            "show" => match (string_arg(args, "title"), string_arg(args, "body")) {
                (Ok(title), Ok(body)) => {
                    if self.post(id, title, body) {
                        call.resolve(&json!({ "posted": true }));
                    } else {
                        call.reject(
                            "unavailable",
                            "`notify-send` is missing or no notification service is running",
                        );
                    }
                }
                (Err(m), _) | (_, Err(m)) => call.reject("invalid_args", &m),
            },
            "schedule" => {
                let at_ms = args.get("at_ms").and_then(Value::as_i64);
                let repeats = ["repeat_zone", "repeat_time"]
                    .iter()
                    .any(|name| args.get(*name).is_some_and(|v| !v.is_null()));
                match (at_ms, string_arg(args, "title"), string_arg(args, "body")) {
                    _ if repeats => call.reject(
                        "unsupported",
                        "repeating reminders need an app that is always running; schedule the next one when this fires",
                    ),
                    (Some(at_ms), Ok(title), Ok(body)) => {
                        let channel = args.get("channel").and_then(Value::as_str).unwrap_or("").to_string();
                        self.schedule(
                            id,
                            &Reminder {
                                at_ms,
                                title: title.to_string(),
                                body: body.to_string(),
                                channel,
                            },
                        );
                        call.resolve(&json!({ "exact": true, "at_ms": at_ms }));
                    }
                    _ => call.reject("invalid_args", "`at_ms`, `title` and `body` are needed"),
                }
            }
            "cancel" => {
                self.cancel(id);
                call.resolve(&Value::Null);
            }
            "pending" => {
                let list: Vec<Value> = self
                    .waiting()
                    .iter()
                    .map(|(id, r)| {
                        json!({
                            "id": id, "at_ms": r.at_ms, "title": r.title, "body": r.body,
                            "channel": r.channel, "repeat_zone": null, "repeat_time": null,
                        })
                    })
                    .collect();
                call.resolve(&json!(list));
            }
            _ => call.reject(
                "unknown_method",
                &format!("notifications has no method `{method}`"),
            ),
        }
    }
}
