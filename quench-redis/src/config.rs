//! Connection settings and the `redis://` URL they are usually read from.

use crate::error::{Error, Result};
use std::time::Duration;

/// One server's address and credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    /// The logical database (`SELECT n`); ignored on a cluster.
    pub db: u32,
    /// `rediss://`: TLS, verifying the certificate against the public and system roots.
    pub tls: bool,
    /// Accept any certificate (development only), with `rediss://...#insecure`.
    pub tls_insecure: bool,
    pub connect_timeout: Duration,
    /// Limit for one command and its reply.
    pub command_timeout: Option<Duration>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 6379,
            username: None,
            password: None,
            db: 0,
            tls: false,
            tls_insecure: false,
            connect_timeout: Duration::from_secs(5),
            command_timeout: Some(Duration::from_secs(30)),
        }
    }
}

fn percent_decode(text: &str) -> Result<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text
                .get(i + 1..i + 3)
                .ok_or_else(|| Error::Config(format!("bad percent escape in `{text}`")))?;
            out.push(
                u8::from_str_radix(hex, 16)
                    .map_err(|_| Error::Config(format!("bad percent escape in `{text}`")))?,
            );
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| Error::Config("percent-decoded text is not UTF-8".into()))
}

impl Config {
    /// Parses `redis://[[user]:password@]host[:port][/db][?connect_timeout=secs&command_timeout=millis]`
    /// (or `rediss://` for TLS). A fragment of `insecure` on a `rediss://` URL disables certificate checks.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] describing the problem.
    pub fn from_url(url: &str) -> Result<Self> {
        let (rest, tls) = if let Some(rest) = url.strip_prefix("rediss://") {
            (rest, true)
        } else if let Some(rest) = url.strip_prefix("redis://") {
            (rest, false)
        } else {
            return Err(Error::Config(
                "the URL must start with redis:// or rediss://".into(),
            ));
        };
        let (rest, fragment) = rest.split_once('#').map_or((rest, ""), |(a, f)| (a, f));
        let (rest, query) = rest.split_once('?').map_or((rest, ""), |(a, q)| (a, q));
        let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
        let (userinfo, hostport) = authority
            .rsplit_once('@')
            .map_or(("", authority), |(u, h)| (u, h));

        let mut config = Self {
            tls,
            tls_insecure: tls && fragment == "insecure",
            ..Self::default()
        };
        if !userinfo.is_empty() {
            let (user, password) = userinfo
                .split_once(':')
                .map_or((userinfo, None), |(u, p)| (u, Some(p)));
            config.username = (!user.is_empty())
                .then(|| percent_decode(user))
                .transpose()?;
            config.password = password.map(percent_decode).transpose()?;
        }
        if !hostport.is_empty() {
            let (host, port) = if let Some(bracketed) = hostport.strip_prefix('[') {
                let (host, after) = bracketed
                    .split_once(']')
                    .ok_or_else(|| Error::Config("unterminated [ in the host".into()))?;
                (host, after.strip_prefix(':'))
            } else {
                hostport
                    .rsplit_once(':')
                    .map_or((hostport, None), |(h, p)| (h, Some(p)))
            };
            if !host.is_empty() {
                config.host = percent_decode(host)?;
            }
            if let Some(port) = port.filter(|p| !p.is_empty()) {
                config.port = port
                    .parse()
                    .map_err(|_| Error::Config(format!("`{port}` is not a port number")))?;
            }
        }
        if !path.is_empty() {
            config.db = path
                .parse()
                .map_err(|_| Error::Config(format!("`{path}` is not a database number")))?;
        }
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            match pair.split_once('=') {
                Some(("connect_timeout", v)) => {
                    config.connect_timeout =
                        Duration::from_secs(v.parse().map_err(|_| {
                            Error::Config(format!("`{v}` is not a number of seconds"))
                        })?);
                }
                Some(("command_timeout", v)) => {
                    config.command_timeout =
                        Some(Duration::from_millis(v.parse().map_err(|_| {
                            Error::Config(format!("`{v}` is not a number of milliseconds"))
                        })?));
                }
                _ => {}
            }
        }
        Ok(config)
    }

    /// `host:port`, bracketing an IPv6 host.
    #[must_use]
    pub fn address(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_url() {
        let c = Config::from_url("rediss://app:p%40ss@cache.example.com:6380/3?connect_timeout=2&command_timeout=1500#insecure").unwrap();
        assert_eq!(
            (
                c.username.as_deref(),
                c.password.as_deref(),
                c.host.as_str(),
                c.port,
                c.db
            ),
            (Some("app"), Some("p@ss"), "cache.example.com", 6380, 3)
        );
        assert!(c.tls && c.tls_insecure);
        assert_eq!(c.connect_timeout, Duration::from_secs(2));
        assert_eq!(c.command_timeout, Some(Duration::from_millis(1500)));
        assert_eq!(c.address(), "cache.example.com:6380");
    }

    #[test]
    fn defaults_and_shapes() {
        let c = Config::from_url("redis://localhost").unwrap();
        assert_eq!(
            (
                c.host.as_str(),
                c.port,
                c.db,
                c.tls,
                c.username.clone(),
                c.password.clone()
            ),
            ("localhost", 6379, 0, false, None, None)
        );
        let c = Config::from_url("redis://:secret@/2").unwrap();
        assert_eq!(
            (
                c.password.as_deref(),
                c.username.clone(),
                c.host.as_str(),
                c.db
            ),
            (Some("secret"), None, "127.0.0.1", 2)
        );
        assert_eq!(
            Config::from_url("redis://[::1]:7000").unwrap().address(),
            "[::1]:7000"
        );
        assert_eq!(Config::from_url("redis://[::1]").unwrap().port, 6379);
        assert!(
            !Config::from_url("redis://h#insecure").unwrap().tls_insecure,
            "insecure only applies to TLS"
        );
        assert_eq!(Config::from_url("redis://u@h:/").unwrap().port, 6379);
        assert!(Config::from_url("redis://h?other=1&flag").is_ok());
    }

    #[test]
    fn bad_urls() {
        for bad in [
            "http://h",
            "h:6379",
            "redis://h:x",
            "redis://h/notanumber",
            "redis://[::1",
            "redis://h?connect_timeout=x",
            "redis://h?command_timeout=x",
            "redis://u%zz@h",
            "redis://u%4@h",
        ] {
            assert!(
                matches!(Config::from_url(bad), Err(Error::Config(_))),
                "{bad}"
            );
        }
        assert!(Config::from_url("redis://u%ff@h").is_err());
    }
}
