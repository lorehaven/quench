//! IANA time zones: the offset from UTC at any instant, daylight saving, and turning local clock readings
//! back into instants. The zone database is built in, so behaviour does not depend on what the host
//! installed; a `TZif` file (for `/etc/localtime`, say) can be read too.
//!
//! ```
//! use quench_core::time::Time;
//! use quench_tz::{Resolved, Zone};
//!
//! let warsaw: Zone = "Europe/Warsaw".parse().unwrap();
//! let noon_utc = Time::from_ymd_hms(2026, 7, 1, 12, 0, 0).unwrap();
//! assert_eq!(warsaw.offset_at(noon_utc).utc_offset, 7200); // CEST
//! assert_eq!(warsaw.civil_at(noon_utc).hour, 14);
//!
//! // 02:30 on the night the clocks go forward does not exist; 02:30 when they go back happens twice.
//! let gap = warsaw.resolve(&quench_core::date::Date::from_ymd(2026, 3, 29).unwrap().at(2, 30, 0));
//! assert!(matches!(gap, Resolved::Gap));
//! ```

mod pack;
mod posix;
mod zone;

use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use quench_core::time::{Civil, Duration, Time};

pub use pack::{Database, pack};
pub use posix::Rule;
pub use zone::{LocalType, Reading, TzifError, ZoneData};

static DATABASE: OnceLock<Database> = OnceLock::new();
static CACHE: OnceLock<Mutex<std::collections::HashMap<&'static str, Arc<ZoneData>>>> =
    OnceLock::new();

fn database() -> &'static Database {
    DATABASE.get_or_init(|| {
        Database::open(include_bytes!("../data/zones.qtz"))
            .expect("the built-in zone database is valid")
    })
}

/// A name that is not in the zone database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownZone(pub String);

impl fmt::Display for UnknownZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown time zone `{}`", self.0)
    }
}

impl std::error::Error for UnknownZone {}

/// The clock offset in force at an instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offset {
    /// Seconds ahead of UTC (negative: behind).
    pub utc_offset: i32,
    /// Whether it is daylight saving time.
    pub is_dst: bool,
    /// `CET`, `CEST`, `+03` ...
    pub abbreviation: String,
}

/// What a local clock reading means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    /// Exactly one instant has this reading.
    Single(Time),
    /// The clocks were turned back: the reading happens twice. `earlier` is the first time round.
    Ambiguous { earlier: Time, later: Time },
    /// The clocks were turned forward over it: the reading never happens.
    Gap,
}

impl Resolved {
    /// The only instant, if there is exactly one.
    #[must_use]
    pub const fn single(self) -> Option<Time> {
        match self {
            Self::Single(t) => Some(t),
            _ => None,
        }
    }

    /// The first instant with this reading, if any.
    #[must_use]
    pub const fn earliest(self) -> Option<Time> {
        match self {
            Self::Single(t) | Self::Ambiguous { earlier: t, .. } => Some(t),
            Self::Gap => None,
        }
    }

    /// The last instant with this reading, if any.
    #[must_use]
    pub const fn latest(self) -> Option<Time> {
        match self {
            Self::Single(t) | Self::Ambiguous { later: t, .. } => Some(t),
            Self::Gap => None,
        }
    }
}

/// A time zone.
#[derive(Clone)]
pub struct Zone {
    name: Arc<str>,
    data: Arc<ZoneData>,
}

impl Zone {
    /// Coordinated Universal Time.
    #[must_use]
    pub fn utc() -> Self {
        Self::fixed("UTC", 0)
    }

    /// A zone that is always `utc_offset` seconds ahead of UTC.
    #[must_use]
    pub fn fixed(name: &str, utc_offset: i32) -> Self {
        let label = if utc_offset == 0 {
            "UTC".to_string()
        } else {
            let sign = if utc_offset < 0 { '-' } else { '+' };
            let abs = utc_offset.unsigned_abs();
            format!("{sign}{:02}{:02}", abs / 3600, abs % 3600 / 60)
        };
        Self {
            name: name.into(),
            data: Arc::new(ZoneData::fixed(utc_offset, &label)),
        }
    }

    /// The zone called `name` in the IANA database, e.g. `Europe/Warsaw` (exact spelling).
    ///
    /// # Errors
    ///
    /// [`UnknownZone`] when there is no such zone.
    pub fn named(name: &str) -> Result<Self, UnknownZone> {
        let db = database();
        let unknown = || UnknownZone(name.to_string());
        let canonical = db.canonical(name).ok_or_else(unknown)?;
        let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
        let cached = cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(canonical)
            .cloned();
        let data = if let Some(data) = cached {
            data
        } else {
            let data = Arc::new(db.zone(canonical).ok_or_else(unknown)?);
            cache
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(canonical, Arc::clone(&data));
            data
        };
        Ok(Self {
            name: canonical.into(),
            data,
        })
    }

    /// A zone read from `TZif` bytes (a file from `/usr/share/zoneinfo`, say), called `name`.
    ///
    /// # Errors
    ///
    /// [`TzifError`] when the bytes are not valid `TZif`.
    pub fn from_tzif(name: &str, bytes: &[u8]) -> Result<Self, TzifError> {
        Ok(Self {
            name: name.into(),
            data: Arc::new(ZoneData::from_tzif(bytes)?),
        })
    }

    /// The zone this machine is set to: the `TZ` variable (`Europe/Warsaw`, `:Europe/Warsaw`, or a POSIX
    /// rule), else `/etc/localtime`, else UTC.
    #[must_use]
    pub fn local() -> Self {
        if let Ok(tz) = std::env::var("TZ") {
            let tz = tz.trim_start_matches(':');
            if !tz.is_empty() {
                if let Ok(zone) = Self::named(tz) {
                    return zone;
                }
                if let Some(rule) = Rule::parse(tz) {
                    return Self::from_rule(tz, rule, tz);
                }
                if let Ok(bytes) = std::fs::read(tz)
                    && let Ok(zone) = Self::from_tzif(tz, &bytes)
                {
                    return zone;
                }
            }
        }
        if let Ok(target) = std::fs::read_link("/etc/localtime")
            && let Some(name) = target
                .to_string_lossy()
                .split_once("zoneinfo/")
                .map(|(_, name)| name.to_string())
            && let Ok(zone) = Self::named(&name)
        {
            return zone;
        }
        if let Ok(bytes) = std::fs::read("/etc/localtime")
            && let Ok(zone) = Self::from_tzif("Local", &bytes)
        {
            return zone;
        }
        Self::utc()
    }

    fn from_rule(name: &str, rule: Rule, text: &str) -> Self {
        let mut data = ZoneData::fixed(0, "UTC");
        data.rule = Some(rule);
        data.rule_text = text.to_string();
        Self {
            name: name.into(),
            data: Arc::new(data),
        }
    }

    /// Every zone name in the built-in database, sorted (including the old aliases such as `US/Pacific`).
    pub fn names() -> impl Iterator<Item = &'static str> {
        database().names()
    }

    /// The version of the built-in database, e.g. `2026e`.
    #[must_use]
    pub fn database_version() -> &'static str {
        database().version()
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The offset in force at `t`.
    #[must_use]
    pub fn offset_at(&self, t: Time) -> Offset {
        let reading = self.data.reading_at(t.timestamp());
        Offset {
            utc_offset: reading.utc_offset,
            is_dst: reading.is_dst,
            abbreviation: reading.abbreviation.to_string(),
        }
    }

    /// The local calendar fields at `t`.
    #[must_use]
    pub fn civil_at(&self, t: Time) -> Civil {
        let offset = self.data.reading_at(t.timestamp()).utc_offset;
        (t + Duration::seconds(i64::from(offset))).civil()
    }

    /// Which instant(s) the local clock reading `local` means in this zone. Sub-second fields carry over.
    #[must_use]
    pub fn resolve(&self, local: &Civil) -> Resolved {
        let Some(as_utc) = local.to_time() else {
            return Resolved::Gap;
        };
        let base = as_utc.timestamp();
        // Offsets in force around that reading: a change within two days either side is the only way the
        // reading can map to more or fewer than one instant.
        let mut candidates: Vec<i32> = Vec::new();
        for day in -2..=2 {
            let offset = self.data.reading_at(base + day * 86_400).utc_offset;
            if !candidates.contains(&offset) {
                candidates.push(offset);
            }
        }
        let mut found: Vec<Time> = candidates
            .iter()
            .filter_map(|&offset| {
                let t = as_utc - Duration::seconds(i64::from(offset));
                (self.data.reading_at(t.timestamp()).utc_offset == offset).then_some(t)
            })
            .collect();
        found.sort_unstable();
        match found.as_slice() {
            [] => Resolved::Gap,
            [only] => Resolved::Single(*only),
            [earlier, .., later] => Resolved::Ambiguous {
                earlier: *earlier,
                later: *later,
            },
        }
    }
}

impl fmt::Display for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl fmt::Debug for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Zone({})", self.name)
    }
}

impl PartialEq for Zone {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Zone {}

impl FromStr for Zone {
    type Err = UnknownZone;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::named(name)
    }
}

#[cfg(test)]
mod tests;
