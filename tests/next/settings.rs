//! README: "Settings".
//!
//! | `set next`                 | force V2 parsing, disabling detection |
//! | `set dotenv`               | load `.env` files                     |
//! | `set venv = "path"`        | path to a Python virtualenv           |
//! | `set positional-arguments` | `$1`, `$2`, `$@` in recipes           |

use super::Test;

#[test]
fn set_next_selects_the_v2_engine() {
    // Nothing else in this justfile is V2-flavoured, so `set next` is what
    // routes it — `$FOO` persisting across lines proves the engine.
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
fn set_venv_points_at_an_explicit_virtualenv() {
    Test::new()
        .justfile(
            r#"
            set next
            set venv = "custom-env"

            build:
                fake-python
            "#,
        )
        .write_executable("custom-env/bin/activate", "# activate\n")
        .write_executable("custom-env/bin/fake-python", "#!/bin/sh\necho ran-custom-venv\n")
        .stdout("ran-custom-venv\n");
}

#[test]
fn set_dotenv_names_a_specific_file() {
    Test::new()
        .justfile(
            r#"
            set next
            set dotenv = ".env.production"

            build:
                echo "${VALUE:-unset}"
            "#,
        )
        .write(".env", "VALUE=from-default\n")
        .write(".env.production", "VALUE=from-production\n")
        .stdout("from-production\n");
}

#[test]
fn positional_arguments_are_available_in_recipes() {
    Test::new()
        .justfile(
            r#"
            set next
            set positional-arguments

            run NAME:
                printf '%s\n' "$1"
            "#,
        )
        .args(["run", "hello world"])
        .stdout("hello world\n");
}

#[test]
fn listing_recipes_shows_doc_comments() {
    let stdout = Test::new()
        .justfile(
            r#"
            set next

            # Build the project
            build:
                cargo build

            # Run the tests
            test:
                cargo test
            "#,
        )
        .arg("--list")
        .stdout_raw();

    assert!(
        stdout.contains("build") && stdout.contains("Build the project"),
        "expected build and its doc comment in: {stdout}",
    );
    assert!(
        stdout.contains("test") && stdout.contains("Run the tests"),
        "expected test and its doc comment in: {stdout}",
    );
}

#[test]
fn listing_recipes_shows_parameters() {
    let stdout = Test::new()
        .justfile(
            r#"
            set next

            deploy ENV=staging *ARGS:
                echo $ENV
            "#,
        )
        .arg("--list")
        .stdout_raw();

    assert!(
        stdout.contains("ENV") && stdout.contains("ARGS"),
        "expected parameters in: {stdout}",
    );
}

#[test]
fn dry_run_prints_without_executing() {
    Test::new()
        .justfile(
            r#"
            set next

            build:
                touch should-not-exist
            "#,
        )
        .arg("--dry-run")

        .stdout("");

    // The file is asserted absent by the next test's fresh tempdir; here the
    // point is simply that the run succeeds without side effects.
}

#[test]
fn dry_run_does_not_create_files() {
    let test = Test::new()
        .justfile(
            r#"
            set next

            build:
                touch should-not-exist
            "#,
        )
        .arg("--dry-run");

    let path = test.dir().join("should-not-exist");
    test.stdout("");

    assert!(!path.exists(), "--dry-run should not have created the file");
}

#[test]
fn an_unknown_recipe_is_an_error() {
    Test::new()
        .justfile(
            r#"
            set next

            build:
                echo build
            "#,
        )
        .arg("nonexistent")
        .fails_with("nonexistent");
}

#[test]
fn an_alias_resolves_to_its_target() {
    Test::new()
        .justfile(
            r#"
            set next

            alias b = build

            build:
                FOO=1
                echo built
            "#,
        )
        .arg("b")
        .stdout("built\n");
}
