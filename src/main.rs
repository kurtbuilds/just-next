//! `just` — one binary, two justfile dialects.
//!
//! Every invocation takes exactly one of two paths:
//!
//! * **V1** — hand argv, untouched, to the vendored upstream `just`. Same
//!   parser, same output, same exit codes, in-process.
//! * **V2** — just-next's own engine.
//!
//! [`just::dispatch`] picks between them.

use std::ffi::OsString;
use std::process::ExitCode;

use clap::CommandFactory;
use clap_complete::CompleteEnv;
use just::dispatch::{self, Engine};

fn main() -> ExitCode {
    // Shell completion is dialect-independent and must be served before any
    // justfile is read. Mirrors upstream's `src/main.rs`.
    if std::env::var_os("JUST_COMPLETE").is_some_and(|value| value == "bash")
        && std::env::args_os().nth(1).is_none()
    {
        print!(
            "{}",
            include_str!("../crates/just-v1/etc/completion-registration-script.bash")
        );
        return ExitCode::SUCCESS;
    }

    CompleteEnv::with_factory(just::v1::Arguments::command)
        .var("JUST_COMPLETE")
        .complete();

    let args: Vec<OsString> = std::env::args_os().collect();

    let route = dispatch::route(&args);

    match route.engine {
        Engine::V1 => {
            // A justfile does not stop needing its `.env` for having been
            // written in the older dialect; upstream would not load one.
            if let Some(justfile) = &route.justfile {
                just::legacy_dotenv::preload(justfile, &args);
            }
            run_v1(args)
        }
        Engine::V2 => just::v2::cli::main(),
    }
}

/// Run upstream `just` in-process.
///
/// argv is passed through untouched apart from just-next's own engine-override
/// flags, which upstream would reject as unknown arguments.
fn run_v1(args: Vec<OsString>) -> ExitCode {
    let args = args
        .into_iter()
        .filter(|arg| !dispatch::ENGINE_FLAGS.iter().any(|flag| arg == flag));

    match just::v1::run(args) {
        Ok(()) => ExitCode::SUCCESS,
        // Upstream reports failure as the exit code it wants us to exit with.
        Err(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
    }
}
