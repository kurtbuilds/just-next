//! README: "Quoting variables".
//!
//! just treats every argument as a single string, so a quoted list does not
//! survive into a recipe — casey/just#208, open since 2017. just-next quotes
//! arguments automatically and keeps lists intact.

use super::Test;

#[test]
fn a_parameter_is_available_as_a_shell_variable() {
    Test::new()
        .justfile(
            r#"
            greet NAME:
                echo $NAME
            "#,
        )
        .args(["greet", "world"])
        .stdout("world\n");
}

#[test]
fn an_argument_containing_spaces_stays_one_argument() {
    // The core of casey/just#208. `printf '%s\n'` prints one line per argument,
    // so a split argument shows up as two lines.
    Test::new()
        .justfile(
            r#"
            run NAME:
                printf '%s\n' $NAME
            "#,
        )
        .args(["run", "hello world"])
        .stdout("hello world\n");
}

#[test]
fn variadic_arguments_stay_separate() {
    Test::new()
        .justfile(
            r#"
            run *ARGS:
                printf '%s\n' $ARGS
            "#,
        )
        .args(["run", "one", "two", "three"])
        .stdout("one\ntwo\nthree\n");
}

#[test]
fn variadic_arguments_preserve_internal_spaces() {
    // Three arguments, two of which contain a space: three lines out.
    Test::new()
        .justfile(
            r#"
            run *ARGS:
                printf '%s\n' $ARGS
            "#,
        )
        .args(["run", "hello world", "foo", "bar baz"])
        .stdout("hello world\nfoo\nbar baz\n");
}

#[test]
fn a_named_parameter_and_a_variadic_combine() {
    // The README's example: `run NAME *ARGS:` / `./program $NAME $ARGS`.
    Test::new()
        .justfile(
            r#"
            run NAME *ARGS:
                printf '%s\n' $NAME $ARGS
            "#,
        )
        .args(["run", "first arg", "second arg", "third"])
        .stdout("first arg\nsecond arg\nthird\n");
}

#[test]
fn an_empty_variadic_expands_to_nothing() {
    Test::new()
        .justfile(
            r#"
            run *ARGS:
                printf '[%s]\n' "$(printf '%s' $ARGS)"
            "#,
        )
        .arg("run")
        .stdout("[]\n");
}

#[test]
fn arguments_with_shell_metacharacters_are_not_interpreted() {
    Test::new()
        .justfile(
            r#"
            run NAME:
                printf '%s\n' $NAME
            "#,
        )
        .args(["run", "a;b|c&d"])
        .stdout("a;b|c&d\n");
}

#[test]
fn arguments_containing_quotes_survive() {
    Test::new()
        .justfile(
            r#"
            run NAME:
                printf '%s\n' $NAME
            "#,
        )
        .args(["run", "it's \"quoted\""])
        .stdout("it's \"quoted\"\n");
}

#[test]
fn a_glob_in_an_argument_is_not_expanded() {
    Test::new()
        .justfile(
            r#"
            run NAME:
                printf '%s\n' $NAME
            "#,
        )
        .write("actual-file.txt", "")
        .args(["run", "*.txt"])
        .stdout("*.txt\n");
}

#[test]
fn a_default_value_is_used_when_an_argument_is_omitted() {
    Test::new()
        .justfile(
            r#"
            deploy ENV=staging:
                echo $ENV
            "#,
        )
        .arg("deploy")
        .stdout("staging\n");
}

#[test]
fn a_default_value_can_be_overridden() {
    Test::new()
        .justfile(
            r#"
            deploy ENV=staging:
                echo $ENV
            "#,
        )
        .args(["deploy", "production"])
        .stdout("production\n");
}

#[test]
fn a_missing_required_argument_is_an_error() {
    Test::new()
        .justfile(
            r#"
            greet NAME:
                echo $NAME
            "#,
        )
        .arg("greet")
        .fails_with("NAME");
}

#[test]
fn a_plus_variadic_requires_at_least_one_argument() {
    Test::new()
        .justfile(
            r#"
            run +ARGS:
                printf '%s\n' $ARGS
            "#,
        )
        .arg("run")
        .fails_with("ARGS");
}

#[test]
fn a_plus_variadic_accepts_several_arguments() {
    Test::new()
        .justfile(
            r#"
            run +ARGS:
                printf '%s\n' $ARGS
            "#,
        )
        .args(["run", "one", "two"])
        .stdout("one\ntwo\n");
}

#[test]
fn positional_arguments_are_available() {
    // Nothing in this recipe is V2-distinctive — `$1` is ordinary shell in
    // either dialect — so it needs `set next` to select the V2 engine.
    Test::new()
        .justfile(
            r#"
            set next

            run NAME:
                printf '%s\n' "$1"
            "#,
        )
        .args(["run", "hello world"])
        .stdout("hello world\n");
}
