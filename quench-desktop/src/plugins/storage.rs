use super::Runner;
use crate::bridge::{Call, Plugin, string_arg};
use quench_serde::{Value, json};
use std::sync::Arc;

/// Secrets in the desktop's keyring through `secret-tool` (`libsecret`: GNOME Keyring, `KWallet`'s Secret Service,
/// `KeePassXC`).
///
/// An item has the attributes `app` (the app's id) and `key`; the value goes to the program on its
/// standard input, never on a command line.
pub struct SecureStorage {
    app: String,
    run: Arc<dyn Runner>,
}

impl std::fmt::Debug for SecureStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureStorage")
            .field("app", &self.app)
            .finish_non_exhaustive()
    }
}

impl SecureStorage {
    /// Storage for the app with this id (the keyring groups items by it).
    #[must_use]
    pub fn new(app: &str, run: Arc<dyn Runner>) -> Self {
        Self {
            app: app.to_string(),
            run,
        }
    }

    fn secret_tool(
        &self,
        args: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<super::Output, (&'static str, String)> {
        self.run.run("secret-tool", args, stdin).map_err(|e| {
            (
                "unavailable",
                format!("the keyring needs `secret-tool` (libsecret-tools) and a running secret service: {e}"),
            )
        })
    }

    fn set(&self, key: &str, value: &str) -> Result<Value, (&'static str, String)> {
        let label = format!("{} {key}", self.app);
        let out = self.secret_tool(
            &["store", "--label", &label, "app", &self.app, "key", key],
            Some(value.as_bytes()),
        )?;
        if out.success {
            Ok(Value::Null)
        } else {
            Err((
                "internal",
                "the keyring refused the secret (is it unlocked?)".to_string(),
            ))
        }
    }

    fn get(&self, key: &str) -> Result<Value, (&'static str, String)> {
        let out = self.secret_tool(&["lookup", "app", &self.app, "key", key], None)?;
        // `lookup` ends with failure and prints nothing when there is no such item.
        Ok(if out.success {
            json!(out.stdout)
        } else {
            Value::Null
        })
    }

    fn remove(&self, key: &str) -> Result<Value, (&'static str, String)> {
        self.secret_tool(&["clear", "app", &self.app, "key", key], None)?;
        Ok(Value::Null)
    }

    fn keys(&self) -> Result<Value, (&'static str, String)> {
        let out = self.secret_tool(&["search", "--all", "app", &self.app], None)?;
        // libsecret's `search` prints the items' attributes on its error output and their secrets on its
        // standard output; the attributes are all this needs, and older versions put them on either.
        let mut keys: Vec<&str> = out
            .stderr
            .lines()
            .chain(out.stdout.lines())
            .filter_map(|line| line.strip_prefix("attribute.key = "))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        Ok(json!(keys))
    }

    fn clear(&self) -> Result<Value, (&'static str, String)> {
        self.secret_tool(&["clear", "app", &self.app], None)?;
        Ok(Value::Null)
    }
}

impl Plugin for SecureStorage {
    fn name(&self) -> &'static str {
        "secure_storage"
    }

    fn call(&self, method: &str, args: &Value, call: Call) {
        let key = || string_arg(args, "key").map_err(|m| ("invalid_args", m));
        let result = match method {
            "set" => key().and_then(|key| {
                string_arg(args, "value")
                    .map_err(|m| ("invalid_args", m))
                    .and_then(|value| self.set(key, value))
            }),
            "get" => key().and_then(|key| self.get(key)),
            "remove" => key().and_then(|key| self.remove(key)),
            "keys" => self.keys(),
            "clear" => self.clear(),
            _ => Err((
                "unknown_method",
                format!("secure_storage has no method `{method}`"),
            )),
        };
        match result {
            Ok(value) => call.resolve(&value),
            Err((code, message)) => call.reject(code, &message),
        }
    }
}
