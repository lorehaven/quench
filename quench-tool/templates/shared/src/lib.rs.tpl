//! What the app and the server share: the types that cross the wire and the functions the app
//! calls. On the server `greet` is the function below; in the browser it is a request to it.

use quench_fullstack::{ServerError, server};
use quench_serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Greeting {
    pub text: String,
}

/// Makes a greeting. A blank name is refused, naming the field.
#[server]
pub async fn greet(name: String) -> Result<Greeting, ServerError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerError::invalid("name", "tell me your name"));
    }
    Ok(Greeting {
        text: format!("Hello, {name}!"),
    })
}
