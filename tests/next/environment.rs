//! README: "Automatic Environment Setup".
//!
//! | `.env` files        | loaded automatically, `.env.local` takes precedence |
//! | `node_modules/.bin` | added to `PATH` if present                          |
//! | `target/debug`      | added to `PATH`, respecting `$CARGO_TARGET_DIR`     |
//! | Python virtualenv   | activated automatically                             |

use super::Test;

/// A justfile with a V2 signal in it, so detection routes to the V2 engine.
const SHOW: &str = "show:\n    FOO=1\n    echo \"${VALUE:-unset}\"\n";

#[test]
fn a_dotenv_file_is_loaded_automatically() {
    Test::new()
        .justfile(SHOW)
        .write(".env", "VALUE=from-dotenv\n")
        .stdout("from-dotenv\n");
}

#[test]
fn dotenv_local_takes_precedence_over_dotenv() {
    Test::new()
        .justfile(SHOW)
        .write(".env", "VALUE=from-dotenv\n")
        .write(".env.local", "VALUE=from-local\n")
        .stdout("from-local\n");
}

#[test]
fn a_real_environment_variable_beats_a_dotenv_file() {
    Test::new()
        .justfile(SHOW)
        .write(".env", "VALUE=from-dotenv\n")
        .env("VALUE", "from-environment")
        .stdout("from-environment\n");
}

#[test]
fn dotenv_comments_and_blank_lines_are_skipped() {
    Test::new()
        .justfile(SHOW)
        .write(".env", "# a comment\n\nVALUE=from-dotenv\n")
        .stdout("from-dotenv\n");
}

#[test]
fn dotenv_values_may_be_quoted() {
    Test::new()
        .justfile(SHOW)
        .write(".env", "VALUE=\"quoted value\"\n")
        .stdout("quoted value\n");
}

#[test]
fn a_dotenv_file_is_found_in_a_parent_directory() {
    Test::new()
        .write("sub/justfile", SHOW)
        .write(".env", "VALUE=from-parent\n")
        .current_dir("sub")
        .stdout("from-parent\n");
}

#[test]
fn node_modules_bin_is_added_to_path() {
    Test::new()
        .justfile("show:\n    FOO=1\n    fake-tool\n")
        .write_executable(
            "node_modules/.bin/fake-tool",
            "#!/bin/sh\necho ran-node-tool\n",
        )
        .stdout("ran-node-tool\n");
}

#[test]
fn node_modules_bin_is_absent_when_the_directory_does_not_exist() {
    let path = Test::new()
        .justfile("show:\n    FOO=1\n    echo $PATH\n")
        .stdout_raw();

    assert!(
        !path.contains("node_modules"),
        "PATH should not mention node_modules: {path}",
    );
}

#[test]
fn cargo_target_debug_is_added_to_path() {
    Test::new()
        .justfile("show:\n    FOO=1\n    fake-binary\n")
        .write_executable("target/debug/fake-binary", "#!/bin/sh\necho ran-cargo-binary\n")
        .stdout("ran-cargo-binary\n");
}

#[test]
fn cargo_target_dir_is_respected() {
    let test = Test::new()
        .justfile("show:\n    FOO=1\n    fake-binary\n")
        .write_executable("custom-target/debug/fake-binary", "#!/bin/sh\necho ran-custom\n");

    let target = test.dir().join("custom-target");
    test.env("CARGO_TARGET_DIR", target.to_str().unwrap())
        .stdout("ran-custom\n");
}

#[test]
fn a_virtualenv_is_activated_automatically() {
    let virtual_env = Test::new()
        .justfile("show:\n    FOO=1\n    echo \"${VIRTUAL_ENV:-unset}\"\n")
        .write_executable(".venv/bin/activate", "# activate script\n")
        .stdout_raw();

    assert!(
        virtual_env.trim().ends_with(".venv"),
        "VIRTUAL_ENV should point at the .venv directory, got: {virtual_env}",
    );
}

#[test]
fn a_virtualenv_bin_directory_is_added_to_path() {
    Test::new()
        .justfile("show:\n    FOO=1\n    fake-python\n")
        .write_executable(".venv/bin/activate", "# activate script\n")
        .write_executable(".venv/bin/fake-python", "#!/bin/sh\necho ran-venv-python\n")
        .stdout("ran-venv-python\n");
}

#[test]
fn a_venv_directory_is_detected() {
    Test::new()
        .justfile("show:\n    FOO=1\n    fake-python\n")
        .write_executable("venv/bin/activate", "# activate script\n")
        .write_executable("venv/bin/fake-python", "#!/bin/sh\necho ran-venv\n")
        .stdout("ran-venv\n");
}

#[test]
fn a_uv_venv_directory_is_detected() {
    Test::new()
        .justfile("show:\n    FOO=1\n    fake-python\n")
        .write_executable(".uv/venv/bin/activate", "# activate script\n")
        .write_executable(".uv/venv/bin/fake-python", "#!/bin/sh\necho ran-uv\n")
        .stdout("ran-uv\n");
}
