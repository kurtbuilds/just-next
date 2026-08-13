//! README: "Backwards Compatibility".
//!
//! just-next detects legacy justfiles and runs them on the vendored upstream
//! engine, so it drops in for `just` without breaking existing files. These
//! tests assert on the *observable* consequence of that routing rather than on
//! the detector itself — `src/dispatch.rs` unit-tests the rules directly.

use super::Test;

/// `{{ }}` interpolation only evaluates under V1.
#[test]
fn colon_equals_assignment_routes_to_v1() {
    Test::new()
        .justfile(
            r#"
            foo := "bar"

            build:
                @echo {{foo}}
            "#,
        )
        .stdout("bar\n");
}

#[test]
fn builtin_functions_route_to_v1() {
    Test::new()
        .justfile(
            r#"
            build:
                @echo {{uppercase("hi")}}
            "#,
        )
        .stdout("HI\n");
}

#[test]
fn attributes_route_to_v1() {
    Test::new()
        .justfile(
            r#"
            [private]
            hidden:
                @echo hidden

            build:
                @echo built
            "#,
        )
        .arg("build")
        .stdout("built\n");
}

#[test]
fn conditionals_route_to_v1() {
    Test::new()
        .justfile(
            r#"
            foo := if "a" == "a" { "yes" } else { "no" }

            build:
                @echo {{foo}}
            "#,
        )
        .stdout("yes\n");
}

#[test]
fn backticks_route_to_v1() {
    Test::new()
        .justfile(
            r#"
            foo := `echo from-backtick`

            build:
                @echo {{foo}}
            "#,
        )
        .stdout("from-backtick\n");
}

#[test]
fn dependency_arguments_route_to_v1() {
    Test::new()
        .justfile(
            r#"
            build: (setup "arg")
                @echo build

            setup WHAT:
                @echo {{WHAT}}
            "#,
        )
        .arg("build")
        .stdout("arg\nbuild\n");
}

#[test]
fn exported_parameters_route_to_v1() {
    // `$FOO` in a recipe header is V1's exported-parameter syntax, not a V2
    // shell reference.
    Test::new()
        .justfile(
            r#"
            wut $FOO='a':
                @echo $FOO
            "#,
        )
        .stdout("a\n");
}

#[test]
fn set_export_disarms_the_parameter_signal() {
    // Under `set export`, `$argument` is ordinary V1: the setting puts every
    // parameter into the environment.
    Test::new()
        .justfile(
            r#"
            set export

            foo argument:
                @echo "$argument"
            "#,
        )
        .args(["foo", "value"])
        .stdout("value\n");
}

#[test]
fn shebang_bodies_do_not_trigger_v2() {
    // A shebang recipe already runs as one script under V1, so a bare
    // assignment in its body is not evidence of V2.
    Test::new()
        .justfile(
            r#"
            foo := "v1"

            build:
                #!/bin/sh
                code=42
                echo $code
            "#,
        )
        .stdout("42\n");
}

#[test]
fn shell_control_flow_does_not_trigger_v1() {
    // Regression: an earlier detector treated a bare `if `/`else ` anywhere in
    // the file as legacy syntax, which caught ordinary shell control flow.
    Test::new()
        .justfile(
            r#"
            deploy TARGET:
                if [ -n "$TARGET" ]; then echo "got $TARGET"; else echo none; fi
            "#,
        )
        .args(["deploy", "prod"])
        .stdout("got prod\n");
}

#[test]
fn set_next_forces_v2_despite_v1_syntax() {
    Test::new()
        .justfile(
            r#"
            set next

            build:
                FOO=bar
                echo $FOO
            "#,
        )
        .stdout("bar\n");
}

#[test]
fn the_legacy_flag_forces_v1() {
    // A justfile that would otherwise route to V2, pinned to the V1 engine.
    // `--dump` is a V1-only flag, so its success also proves which engine ran.
    Test::new()
        .justfile(
            r#"
            build:
                FOO=bar
                echo $FOO
            "#,
        )
        .args(["--legacy", "--dump"])
        .stdout("build:\n    FOO=bar\n    echo $FOO\n");
}

#[test]
fn v1_justfiles_keep_upstream_error_messages() {
    Test::new()
        .justfile(
            r#"
            foo := "bar"

            build:
                @echo {{foo}}
            "#,
        )
        .arg("nonexistent")
        .fails_with("justfile does not contain recipe `nonexistent`");
}

#[test]
fn v1_justfiles_keep_upstream_exit_codes() {
    Test::new()
        .justfile(
            r#"
            foo := "bar"

            build:
                @exit 3
            "#,
        )
        .fails_with("exit code 3");
}

/// The point of routing ambiguous justfiles to V2: a file with no dialect
/// signal at all — every recipe a bare command — still gets the automatic
/// environment setup. Before, such a file fell back to V1 and silently ran
/// without its `.env`.
#[test]
fn a_signal_free_justfile_gets_the_v2_environment() {
    Test::new()
        .justfile(
            r#"
            build:
                echo "${VALUE:-unset}"
            "#,
        )
        .write(".env", "VALUE=from-dotenv\n")
        .stdout("from-dotenv\n");
}

/// The engine choice for an ambiguous file also depends on the invocation, not
/// only on the justfile: V2 implements a handful of flags, so anything outside
/// them has to reach V1 or it would die on "unexpected argument".
#[test]
fn a_v1_only_flag_routes_an_ambiguous_justfile_to_v1() {
    Test::new()
        .justfile(
            r#"
            build:
                echo hi
            "#,
        )
        .arg("--dump")
        .stdout("build:\n    echo hi\n");
}

/// The flags V2 does implement leave it in charge, so they keep the automatic
/// environment setup.
#[test]
fn a_v2_flag_keeps_an_ambiguous_justfile_on_v2() {
    Test::new()
        .justfile(
            r#"
            build:
                echo "${VALUE:-unset}"
            "#,
        )
        .write(".env", "VALUE=from-dotenv\n")
        .arg("--dry-run")
        .stdout("");
}
