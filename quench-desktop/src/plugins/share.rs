use super::Runner;
use crate::bridge::{Call, Plugin, string_arg};
use quench_serde::Value;
use std::fmt::Write as _;
use std::sync::Arc;

/// Opening things in the desktop's apps, with `xdg-open`. There is no share sheet on a desktop.
///
/// `text` composes a mail (`mailto:`), `file` and `open` hand a `file://` URI to the app the system chooses for it. An address
/// that is not a local file or a web link is refused.
pub struct Share {
    run: Arc<dyn Runner>,
}

impl std::fmt::Debug for Share {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Share").finish_non_exhaustive()
    }
}

impl Share {
    #[must_use]
    pub fn new(run: Arc<dyn Runner>) -> Self {
        Self { run }
    }

    fn open(&self, target: &str) -> Result<Value, (&'static str, String)> {
        // `xdg-open` takes options, so nothing that could be one goes to it.
        let fine = ["file://", "https://", "http://", "mailto:"]
            .iter()
            .any(|p| target.starts_with(p));
        if !fine {
            return Err((
                "invalid_args",
                "only file, http(s) and mailto addresses can be opened".to_string(),
            ));
        }
        self.run
            .spawn("xdg-open", &[target])
            .map(|()| Value::Null)
            .map_err(|e| {
                (
                    "unavailable",
                    format!("`xdg-open` could not be started: {e}"),
                )
            })
    }
}

/// Percent-encodes text for a `mailto:` URI.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

impl Plugin for Share {
    fn name(&self) -> &'static str {
        "share"
    }

    fn call(&self, method: &str, args: &Value, call: Call) {
        let result = match method {
            "text" => string_arg(args, "text")
                .map_err(|m| ("invalid_args", m))
                .and_then(|text| {
                    let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                    self.open(&format!(
                        "mailto:?subject={}&body={}",
                        encode(title),
                        encode(text)
                    ))
                }),
            "file" | "open" => string_arg(args, "uri")
                .map_err(|m| ("invalid_args", m))
                .and_then(|uri| self.open(uri)),
            _ => Err(("unknown_method", format!("share has no method `{method}`"))),
        };
        match result {
            Ok(value) => call.resolve(&value),
            Err((code, message)) => call.reject(code, &message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn mailto_text_is_percent_encoded() {
        assert_eq!(encode("a b&c=é\n"), "a%20b%26c%3D%C3%A9%0A");
        assert_eq!(encode("Az09-_.~"), "Az09-_.~");
    }
}
