//! README: "Export Statements" and "Export Within Recipes".
//!
//! just spells a top-level export `export FOO := "bar"` and cannot persist an
//! `export` across recipe lines without a shebang. just-next uses shell syntax
//! for both.

use super::Test;

#[test]
fn shell_style_export_sets_an_environment_variable() {
    Test::new()
        .justfile(
            r#"
            export FOO="bar"

            build:
                echo $FOO
            "#,
        )
        .stdout("bar\n");
}

#[test]
fn export_can_reference_an_existing_environment_variable() {
    // The README's headline example: prepending to PATH without `env_var()`.
    Test::new()
        .justfile(
            r#"
            export GREETING="hello $NAME"

            build:
                echo $GREETING
            "#,
        )
        .env("NAME", "world")
        .stdout("hello world\n");
}

#[test]
fn export_prepends_to_path() {
    let path = Test::new()
        .justfile(
            r#"
            export PATH="/injected/bin:$PATH"

            build:
                echo $PATH
            "#,
        )
        .stdout_raw();

    assert!(
        path.starts_with("/injected/bin:"),
        "PATH should start with the injected directory, got: {path}",
    );
    assert!(
        path.trim().len() > "/injected/bin:".len(),
        "the previous PATH should still be there, got: {path}",
    );
}

#[test]
fn multiple_exports_are_all_applied() {
    Test::new()
        .justfile(
            r#"
            export ONE="1"
            export TWO="2"

            build:
                echo $ONE$TWO
            "#,
        )
        .stdout("12\n");
}

#[test]
fn unquoted_export_values_work() {
    Test::new()
        .justfile(
            r#"
            export FOO=bar

            build:
                echo $FOO
            "#,
        )
        .stdout("bar\n");
}

#[test]
fn export_inside_a_recipe_persists_to_later_lines() {
    // Under just this needs a shebang, because each line is its own shell.
    Test::new()
        .justfile(
            r#"
            build:
                export CC=clang
                echo $CC
            "#,
        )
        .stdout("clang\n");
}

#[test]
fn export_inside_a_recipe_reaches_child_processes() {
    Test::new()
        .justfile(
            r#"
            build:
                export CC=clang
                sh -c 'echo $CC'
            "#,
        )
        .stdout("clang\n");
}

#[test]
fn recipe_export_can_reference_an_earlier_assignment() {
    Test::new()
        .justfile(
            r#"
            build:
                VERSION=1.2.3
                export TAG="v$VERSION"
                echo $TAG
            "#,
        )
        .stdout("v1.2.3\n");
}
