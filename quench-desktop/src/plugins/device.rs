use crate::bridge::{Call, Emitter, Plugin};
use quench_serde::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// How often `watch` looks for a change.
const WATCH_EVERY: Duration = Duration::from_secs(5);

/// The machine's state, from `/sys`: the network, the battery, the power profile. `watch` pushes a `state` event
/// whenever it changes.
#[derive(Debug)]
pub struct DeviceState {
    sys: PathBuf,
    every: Duration,
    /// Bumped by `watch` and `unwatch`; a watcher stops when it is not the current one.
    generation: Arc<AtomicU64>,
    emitter: Mutex<Option<Emitter>>,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self::at(Path::new("/sys"), WATCH_EVERY)
    }
}

impl DeviceState {
    /// Reads `sys` (a `/sys`-shaped tree) and looks for changes every `every`.
    #[must_use]
    pub fn at(sys: &Path, every: Duration) -> Self {
        Self {
            sys: sys.to_path_buf(),
            every,
            generation: Arc::new(AtomicU64::new(0)),
            emitter: Mutex::new(None),
        }
    }

    /// The state now, as the bridge reports it.
    #[must_use]
    pub fn state(&self) -> Value {
        read_state(&self.sys)
    }
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

fn names(dir: &Path, prefix: &str) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(prefix))
        .collect();
    found.sort();
    found
}

/// What kind of link an interface is, by its name.
fn transport_of(name: &str) -> &'static str {
    if name.starts_with("wl") {
        "wifi"
    } else if name.starts_with("ww") {
        "cellular"
    } else if name.starts_with("en") || name.starts_with("eth") {
        "ethernet"
    } else {
        "other"
    }
}

fn read_state(sys: &Path) -> Value {
    // The first interface that is up, besides the loopback and virtual ones.
    let net = sys.join("class/net");
    let link = names(&net, "")
        .into_iter()
        .filter(|name| name != "lo")
        .find(|name| {
            // `device` exists for hardware, not for bridges, tunnels and containers' links.
            read(&net.join(name).join("operstate")).as_deref() == Some("up")
                && net.join(name).join("device").exists()
        });
    let (online, transport) = link
        .as_deref()
        .map_or((false, "none"), |name| (true, transport_of(name)));

    let power = sys.join("class/power_supply");
    let battery = names(&power, "BAT")
        .into_iter()
        .next()
        .map(|name| power.join(name));
    let percent = battery
        .as_ref()
        .and_then(|dir| read(&dir.join("capacity")))
        .and_then(|text| text.parse::<i64>().ok())
        .unwrap_or(-1);
    let status = battery.as_ref().and_then(|dir| read(&dir.join("status")));
    let on_mains = names(&power, "AC")
        .into_iter()
        .chain(names(&power, "ADP"))
        .any(|name| read(&power.join(name).join("online")).as_deref() == Some("1"));
    let charging =
        matches!(status.as_deref(), Some("Charging" | "Full")) || (battery.is_none() && on_mains);
    let profile = read(&sys.join("firmware/acpi/platform_profile"));
    json!({
        "online": online,
        "transport": transport,
        "metered": false,
        "battery_percent": percent,
        "charging": charging,
        "battery_low": (0..=15).contains(&percent) && !charging,
        "power_save": profile.as_deref() == Some("low-power"),
    })
}

impl Plugin for DeviceState {
    fn name(&self) -> &'static str {
        "device"
    }

    fn call(&self, method: &str, _args: &Value, call: Call) {
        match method {
            "state" => call.resolve(&self.state()),
            "watch" => {
                let mine = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
                let emitter = self
                    .emitter
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                if let Some(emitter) = emitter {
                    let (sys, every, generation) =
                        (self.sys.clone(), self.every, Arc::clone(&self.generation));
                    std::thread::spawn(move || {
                        let mut last = read_state(&sys);
                        while generation.load(Ordering::SeqCst) == mine {
                            std::thread::sleep(every);
                            let now = read_state(&sys);
                            if now != last && generation.load(Ordering::SeqCst) == mine {
                                emitter.emit("device", "state", &now);
                                last = now;
                            }
                        }
                    });
                }
                call.resolve(&Value::Null);
            }
            "unwatch" => {
                self.generation.fetch_add(1, Ordering::SeqCst);
                call.resolve(&Value::Null);
            }
            _ => call.reject(
                "unknown_method",
                &format!("device has no method `{method}`"),
            ),
        }
    }

    fn attach(&self, emitter: Emitter) {
        *self.emitter.lock().unwrap_or_else(PoisonError::into_inner) = Some(emitter);
    }
}
