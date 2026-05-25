//! sqlglot-equivalent classifier. Tags a statement as read / write / ddl
//! and lists the tables it touches.

#![allow(
    unreachable_pub,
    clippy::todo,
    clippy::unimplemented,
    dead_code,
    unused_variables
)]

pub struct Classification {
    pub op: &'static str, // "read" | "write" | "ddl"
    pub tables: Vec<String>,
}

/// In a real impl, parse via `sqlparser-rs` or similar.
pub fn classify(_sql: &str) -> Classification {
    todo!()
}
