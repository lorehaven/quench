use crate::bridge::{Call, Plugin, string_arg};
use quench_serde::{Value, json};

/// A plugin that exists to try the bridge: it returns what it is given, says what machine it runs on, adds, and
/// fails on request. The gallery's *Native* tab calls it; leave it out of a real app.
#[derive(Debug, Default, Clone, Copy)]
pub struct Echo;

impl Plugin for Echo {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn call(&self, method: &str, args: &Value, call: Call) {
        match method {
            "say" => match string_arg(args, "text") {
                Ok(text) => call.resolve(&json!(text)),
                Err(message) => call.reject("invalid_args", &message),
            },
            // `sdk` is an Android notion; here it is 0 and the model is the machine's name.
            "device" => call.resolve(&json!({ "sdk": 0, "model": hostname() })),
            "add" => match (
                args.get("a").and_then(Value::as_i64),
                args.get("b").and_then(Value::as_i64),
            ) {
                (Some(a), Some(b)) => call.resolve(&json!(a + b)),
                _ => call.reject("invalid_args", "`a` and `b` are needed, as integers"),
            },
            "fail" => {
                let code = args.get("code").and_then(Value::as_str).unwrap_or("failed");
                call.reject(code, "asked to fail");
            }
            // A desktop has no permission dialog for notifications.
            "notifications" => call.resolve(&json!(true)),
            _ => call.reject("unknown_method", &format!("echo has no method `{method}`")),
        }
    }
}

/// The machine's name.
fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_string())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "linux".to_string())
}
