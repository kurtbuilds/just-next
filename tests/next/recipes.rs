//! README: "Variable Assignments in Recipes".
//!
//! just runs each recipe line in its own shell, so a variable assigned on one
//! line is gone by the next unless the recipe has a shebang. just-next keeps an
//! environment that persists across lines.

use super::Test;

#[test]
fn an_assignment_persists_to_the_next_line() {
    // The README's example, verbatim — no shebang.
    Test::new()
        .justfile(
            r#"
            build:
                FOO=$(echo bar)
                echo $FOO
            "#,
        )
        .stdout("bar\n");
}

#[test]
fn a_literal_assignment_persists() {
    Test::new()
        .justfile(
            r#"
            build:
                FOO=bar
                echo $FOO
            "#,
        )
        .stdout("bar\n");
}

#[test]
fn assignments_persist_across_many_lines() {
    Test::new()
        .justfile(
            r#"
            build:
                ONE=1
                TWO=2
                echo $ONE
                echo $TWO
                echo $ONE$TWO
            "#,
        )
        .stdout("1\n2\n12\n");
}

#[test]
fn a_later_assignment_overwrites_an_earlier_one() {
    Test::new()
        .justfile(
            r#"
            build:
                FOO=first
                FOO=second
                echo $FOO
            "#,
        )
        .stdout("second\n");
}

#[test]
fn an_assignment_can_reference_an_earlier_one() {
    Test::new()
        .justfile(
            r#"
            build:
                NAME=world
                GREETING="hello $NAME"
                echo $GREETING
            "#,
        )
        .stdout("hello world\n");
}

#[test]
fn assignments_reach_child_processes() {
    Test::new()
        .justfile(
            r#"
            build:
                FOO=bar
                sh -c 'echo $FOO'
            "#,
        )
        .stdout("bar\n");
}

#[test]
fn a_value_may_contain_spaces_when_quoted() {
    Test::new()
        .justfile(
            r#"
            build:
                BAR="x/src y/src"
                sh -c 'echo $BAR'
            "#,
        )
        .stdout("x/src y/src\n");
}

#[test]
fn a_command_substitution_may_contain_spaces() {
    Test::new()
        .justfile(
            r#"
            build:
                FOO=$(echo a b)
                echo $FOO
            "#,
        )
        .stdout("a b\n");
}

#[test]
fn several_assignments_on_one_line_all_persist() {
    Test::new()
        .justfile(
            r#"
            build:
                ONE=1 TWO="a b"
                echo $ONE
                sh -c 'echo $TWO'
            "#,
        )
        .stdout("1\na b\n");
}

/// A line with a command after the assignments is a command with an environment
/// prefix, not an assignment. Taking it as one swallowed the command whole: its
/// value was evaluated for its output, so stdout became the assigned value and
/// stderr and the exit status were discarded.
#[test]
fn an_environment_prefix_runs_the_command() {
    Test::new()
        .justfile(
            r#"
            build:
                FOO=bar ONE="a b" sh -c 'echo "$FOO $ONE"'
            "#,
        )
        .stdout("bar a b\n");
}

#[test]
fn an_environment_prefix_does_not_persist() {
    // Shell scoping: FOO is set for that command only.
    Test::new()
        .justfile(
            r#"
            build:
                FOO=bar true
                echo "[${FOO:-unset}]"
            "#,
        )
        .stdout("[unset]\n");
}

#[test]
fn an_environment_prefix_fails_the_recipe() {
    Test::new()
        .justfile(
            r#"
            build:
                FOO=bar false
                echo unreachable
            "#,
        )
        .fails_with("");
}

#[test]
fn an_environment_prefix_leaves_stderr_alone() {
    Test::new()
        .justfile(
            r#"
            build:
                FOO=bar sh -c 'echo complaint >&2; exit 1'
            "#,
        )
        .fails_with("complaint");
}

#[test]
fn shebang_recipes_still_run_as_one_script() {
    // Shebang recipes keep their existing meaning: the whole body is one script.
    Test::new()
        .justfile(
            r#"
            set next

            build:
                #!/bin/sh
                FOO=bar
                echo $FOO
            "#,
        )
        .stdout("bar\n");
}

#[test]
fn a_failing_line_fails_the_recipe() {
    Test::new()
        .justfile(
            r#"
            build:
                FOO=bar
                false
                echo unreachable
            "#,
        )
        .fails_with("");
}

#[test]
fn a_dash_prefix_ignores_a_failing_line() {
    Test::new()
        .justfile(
            r#"
            build:
                FOO=bar
                -false
                echo $FOO
            "#,
        )
        .stdout("bar\n");
}

#[test]
fn dependencies_run_before_the_recipe() {
    Test::new()
        .justfile(
            r#"
            build: setup
                echo build

            setup:
                echo setup
            "#,
        )
        .arg("build")
        .stdout("setup\nbuild\n");
}

#[test]
fn each_recipe_gets_a_fresh_environment() {
    // State persists within a recipe, not across recipes.
    Test::new()
        .justfile(
            r#"
            build: setup
                echo "[${FOO:-unset}]"

            setup:
                FOO=bar
                echo "[$FOO]"
            "#,
        )
        .arg("build")
        .stdout("[bar]\n[unset]\n");
}

#[test]
fn the_first_recipe_is_the_default() {
    Test::new()
        .justfile(
            r#"
            first:
                FOO=1
                echo first

            second:
                echo second
            "#,
        )
        .stdout("first\n");
}
