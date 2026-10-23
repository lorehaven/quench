//! Connection settings and the `postgres://` URL they are usually read from.

use crate::error::{Error, Result};
use std::time::Duration;

/// How TLS is used (libpq's `sslmode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SslMode {
    /// Plain text only.
    Disable,
    /// Try TLS, fall back to plain text if the server declines. The certificate is not verified.
    #[default]
    Prefer,
    /// TLS or fail. The certificate is not verified.
    Require,
    /// TLS or fail, and verify the certificate chain and the server name.
    VerifyFull,
}

impl SslMode {
    fn parse(text: &str) -> Result<Self> {
        match text {
            "disable" => Ok(Self::Disable),
            "allow" | "prefer" => Ok(Self::Prefer),
            "require" => Ok(Self::Require),
            // The server name is always checked, so `verify-ca` is as strict as `verify-full`.
            "verify-ca" | "verify-full" => Ok(Self::VerifyFull),
            other => Err(Error::Config(format!("unknown sslmode `{other}`"))),
        }
    }
}

/// Everything needed to open a connection.
#[derive(Debug, Clone)]
pub struct Config {
    /// A hostname or IP address, or the directory of a Unix socket when it starts with `/`.
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
    pub ssl_mode: SslMode,
    /// PEM certificate(s) to trust in addition to the public roots (with `VerifyFull`).
    pub ssl_root_cert_pem: Vec<Vec<u8>>,
    pub application_name: Option<String>,
    /// The `options` startup parameter, e.g. `-c search_path=app`.
    pub options: Option<String>,
    pub connect_timeout: Duration,
    /// Applied to every statement on the client side; the connection is dropped on expiry.
    pub statement_timeout: Option<Duration>,
    /// Largest message accepted from the server.
    pub max_message_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            user: "postgres".to_string(),
            password: None,
            database: None,
            ssl_mode: SslMode::default(),
            ssl_root_cert_pem: Vec::new(),
            application_name: None,
            options: None,
            connect_timeout: Duration::from_secs(10),
            statement_timeout: None,
            max_message_size: 256 * 1024 * 1024,
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
    String::from_utf8(out)
        .map_err(|_| Error::Config("percent-decoded text is not UTF-8".to_string()))
}

impl Config {
    /// Parses `postgres://user:password@host:port/database?sslmode=require&application_name=x`.
    ///
    /// The scheme may be `postgres` or `postgresql`. User, password, database and query values are
    /// percent-decoded; an IPv6 host is written `[::1]`. Recognised query parameters: `sslmode`,
    /// `sslrootcert` (a file path), `application_name`, `options`, `connect_timeout` (seconds),
    /// `statement_timeout` (milliseconds). Unknown ones are ignored, as libpq does for its own extras.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] describing what is wrong.
    pub fn from_url(url: &str) -> Result<Self> {
        let rest = url
            .strip_prefix("postgres://")
            .or_else(|| url.strip_prefix("postgresql://"))
            .ok_or_else(|| {
                Error::Config("the URL must start with postgres:// or postgresql://".to_string())
            })?;
        let (authority_and_path, query) = rest.split_once('?').map_or((rest, ""), |(a, q)| (a, q));
        let (authority, path) = authority_and_path
            .split_once('/')
            .map_or((authority_and_path, ""), |(a, p)| (a, p));
        let (userinfo, hostport) = authority
            .rsplit_once('@')
            .map_or(("", authority), |(u, h)| (u, h));

        let mut config = Self::default();
        if !userinfo.is_empty() {
            let (user, password) = userinfo
                .split_once(':')
                .map_or((userinfo, None), |(u, p)| (u, Some(p)));
            if !user.is_empty() {
                config.user = percent_decode(user)?;
            }
            config.password = password.map(percent_decode).transpose()?;
        }
        if !hostport.is_empty() {
            let (host, port) = if let Some(bracketed) = hostport.strip_prefix('[') {
                let (host, after) = bracketed
                    .split_once(']')
                    .ok_or_else(|| Error::Config("unterminated [ in the host".to_string()))?;
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
            config.database = Some(percent_decode(path)?);
        }
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = percent_decode(value)?;
            match key {
                "sslmode" => config.ssl_mode = SslMode::parse(&value)?,
                "sslrootcert" => {
                    config
                        .ssl_root_cert_pem
                        .push(std::fs::read(&value).map_err(|e| {
                            Error::Config(format!("cannot read sslrootcert {value}: {e}"))
                        })?);
                }
                "application_name" => config.application_name = Some(value),
                "options" => config.options = Some(value),
                "connect_timeout" => {
                    config.connect_timeout = Duration::from_secs(value.parse().map_err(|_| {
                        Error::Config(format!("`{value}` is not a number of seconds"))
                    })?);
                }
                "statement_timeout" => {
                    config.statement_timeout =
                        Some(Duration::from_millis(value.parse().map_err(|_| {
                            Error::Config(format!("`{value}` is not a number of milliseconds"))
                        })?));
                }
                _ => {}
            }
        }
        Ok(config)
    }

    /// The `user` / `database` / ... pairs for the startup message.
    pub(crate) fn startup_params(&self) -> Vec<(String, String)> {
        let mut params = vec![
            ("user".to_string(), self.user.clone()),
            (
                "database".to_string(),
                self.database.clone().unwrap_or_else(|| self.user.clone()),
            ),
            ("client_encoding".to_string(), "UTF8".to_string()),
            // Text results are converted by the client, which assumes UTC timestamps and ISO dates.
            ("TimeZone".to_string(), "UTC".to_string()),
            ("DateStyle".to_string(), "ISO, MDY".to_string()),
        ];
        if let Some(name) = &self.application_name {
            params.push(("application_name".to_string(), name.clone()));
        }
        if let Some(options) = &self.options {
            params.push(("options".to_string(), options.clone()));
        }
        params
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_url() {
        let c = Config::from_url("postgres://bob:s3cr%40t@db.example.com:5433/app?sslmode=require&application_name=svc&connect_timeout=3&statement_timeout=2500&options=-c%20search_path%3Dx").unwrap();
        assert_eq!(
            (
                c.user.as_str(),
                c.password.as_deref(),
                c.host.as_str(),
                c.port
            ),
            ("bob", Some("s3cr@t"), "db.example.com", 5433)
        );
        assert_eq!(c.database.as_deref(), Some("app"));
        assert_eq!(c.ssl_mode, SslMode::Require);
        assert_eq!(c.application_name.as_deref(), Some("svc"));
        assert_eq!(c.connect_timeout, Duration::from_secs(3));
        assert_eq!(c.statement_timeout, Some(Duration::from_millis(2500)));
        assert_eq!(c.options.as_deref(), Some("-c search_path=x"));
    }

    #[test]
    fn defaults_fill_the_gaps() {
        let c = Config::from_url("postgresql://localhost").unwrap();
        assert_eq!(
            (
                c.user.as_str(),
                c.port,
                c.database.clone(),
                c.password.clone(),
                c.ssl_mode
            ),
            ("postgres", 5432, None, None, SslMode::Prefer)
        );
        let c = Config::from_url("postgres://:pw@/db").unwrap();
        assert_eq!(
            (
                c.user.as_str(),
                c.host.as_str(),
                c.password.as_deref(),
                c.database.as_deref()
            ),
            ("postgres", "localhost", Some("pw"), Some("db"))
        );
        let c = Config::from_url("postgres://u@h:/").unwrap();
        assert_eq!((c.port, c.database), (5432, None));
    }

    #[test]
    fn hosts_in_all_their_forms() {
        assert_eq!(
            Config::from_url("postgres://u@[::1]:6000/d").unwrap().host,
            "::1"
        );
        assert_eq!(Config::from_url("postgres://u@[::1]/d").unwrap().port, 5432);
        assert_eq!(
            Config::from_url("postgres://u@%2Fvar%2Frun%2Fpostgresql/d")
                .unwrap()
                .host,
            "/var/run/postgresql"
        );
        assert!(Config::from_url("postgres://u@[::1/d").is_err());
    }

    #[test]
    fn ssl_modes() {
        for (text, mode) in [
            ("disable", SslMode::Disable),
            ("allow", SslMode::Prefer),
            ("prefer", SslMode::Prefer),
            ("require", SslMode::Require),
            ("verify-ca", SslMode::VerifyFull),
            ("verify-full", SslMode::VerifyFull),
        ] {
            assert_eq!(
                Config::from_url(&format!("postgres://h/d?sslmode={text}"))
                    .unwrap()
                    .ssl_mode,
                mode
            );
        }
        assert!(Config::from_url("postgres://h/d?sslmode=bogus").is_err());
    }

    #[test]
    fn bad_urls_are_errors() {
        for bad in [
            "mysql://h/d",
            "h/d",
            "postgres://u@h:port/d",
            "postgres://u@h/d?connect_timeout=soon",
            "postgres://u@h/d?statement_timeout=x",
            "postgres://u%zz@h/d",
            "postgres://u%4@h/d",
            "postgres://h/d?sslrootcert=/no/such/file.pem",
        ] {
            assert!(
                matches!(Config::from_url(bad), Err(Error::Config(_))),
                "{bad}"
            );
        }
        assert!(
            Config::from_url("postgres://u%ff@h/d").is_err(),
            "invalid UTF-8 after decoding"
        );
        assert!(
            Config::from_url("postgres://h/d?unknown=1&&flag").is_ok(),
            "unknown parameters are ignored"
        );
    }

    #[test]
    fn root_certificates_are_read_from_a_file() {
        let dir = quench_core::tmp::TempDir::new().unwrap();
        let path = dir.path().join("root.pem");
        std::fs::write(&path, b"pem bytes").unwrap();
        let c =
            Config::from_url(&format!("postgres://h/d?sslrootcert={}", path.display())).unwrap();
        assert_eq!(c.ssl_root_cert_pem, vec![b"pem bytes".to_vec()]);
    }

    #[test]
    fn startup_parameters() {
        let mut c = Config {
            user: "u".into(),
            ..Config::default()
        };
        let params = c.startup_params();
        assert!(params.contains(&("user".to_string(), "u".to_string())));
        assert!(
            params.contains(&("database".to_string(), "u".to_string())),
            "the database defaults to the user name"
        );
        assert!(params.contains(&("TimeZone".to_string(), "UTC".to_string())));
        c.database = Some("d".into());
        c.application_name = Some("app".into());
        c.options = Some("-c x=1".into());
        let params = c.startup_params();
        assert!(params.contains(&("database".to_string(), "d".to_string())));
        assert!(params.contains(&("application_name".to_string(), "app".to_string())));
        assert!(params.contains(&("options".to_string(), "-c x=1".to_string())));
    }
}
