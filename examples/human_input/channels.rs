//! Channel adapters: ntfy / email / slack. Each adapter dispatches the
//! prompt over its medium, parses the answer back into JSON.

#![allow(
    unreachable_pub,
    clippy::todo,
    clippy::unimplemented,
    dead_code,
    unused_variables
)]

use serde_json::Value;

pub async fn ntfy_phone(_prompt: &str, _schema: &Value) -> Value {
    todo!()
}

pub async fn email_oncall(_prompt: &str, _schema: &Value) -> Value {
    todo!()
}

pub async fn slack_ops(_prompt: &str, _schema: &Value) -> Value {
    todo!()
}

pub async fn dispatch(dest: &str, prompt: &str, schema: &Value) -> Value {
    match dest {
        "ntfy:phone" => ntfy_phone(prompt, schema).await,
        "email:oncall" => email_oncall(prompt, schema).await,
        "slack:ops" => slack_ops(prompt, schema).await,
        _ => todo!(),
    }
}
