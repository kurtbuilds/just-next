//! `just` — a drop-in replacement for [casey/just](https://github.com/casey/just)
//! that fixes long-standing ergonomic issues.
//!
//! A single binary serves two justfile dialects:
//!
//! * **V1** — upstream `just`, vendored as the [`just_v1`] crate and called
//!   in-process. Byte-for-byte compatible: same parser, same output, same exit
//!   codes. No subprocess, no second binary on `PATH`.
//! * **V2** — just-next's own engine ([`v2`]), with the simplified syntax the
//!   README describes.
//!
//! [`dispatch`] decides which engine a given justfile gets, by sniffing its
//! syntax. See that module for the rules.

pub mod dispatch;
pub mod legacy_dotenv;
pub mod search;
pub mod v2;

/// Upstream `just`, vendored verbatim. See `crates/just-v1/`.
pub use just_v1 as v1;

// Re-exported so the vendored upstream test suite — which imports these from
// the `just` crate — compiles unmodified against this crate.
pub use just_v1::{Arguments, INIT_JUSTFILE, Response, unindent};

pub use dispatch::{Engine, detect};
