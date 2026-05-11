//! Two LLM stubs: a generator that proposes patches and a reviewer with
//! a veto. In production these are separate models, possibly different
//! providers, on different runtimes.

#![allow(
    unreachable_pub,
    clippy::todo,
    clippy::unimplemented,
    dead_code,
    unused_variables
)]

use arcp::Envelope;

pub struct Patch {
    pub diff: String,
}

pub struct ReviewVerdict {
    pub grant: bool,
    pub reason: String,
}

pub async fn propose(_ticket: &str, _prior_denial: Option<&str>) -> Patch {
    todo!()
}

pub async fn review(_ticket: &str, _request: &Envelope) -> ReviewVerdict {
    todo!()
}
