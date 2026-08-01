//! README: "Running Recipes in Another Folder".
//!
//! `just api/build` runs `build` from `api/justfile`, with `api/` as the
//! working directory. Unlike the normal justfile search, the path is used as
//! given and never walks up to a parent.

use super::Test;

#[test]
fn a_folder_prefixed_recipe_runs_from_that_folder() {
    Test::new()
        .write(
            "api/justfile",
            "build:\n    FOO=api\n    echo $FOO\n",
        )
        .write("justfile", "build:\n    FOO=root\n    echo $FOO\n")
        .arg("api/build")
        .stdout("api\n");
}

#[test]
fn nested_folder_paths_work() {
    Test::new()
        .write(
            "crates/web/justfile",
            "serve:\n    FOO=web\n    echo $FOO\n",
        )
        .write("justfile", "build:\n    FOO=root\n    echo $FOO\n")
        .arg("crates/web/serve")
        .stdout("web\n");
}

#[test]
fn a_trailing_slash_runs_the_folders_default_recipe() {
    Test::new()
        .write(
            "api/justfile",
            "first:\n    FOO=1\n    echo first\n\nsecond:\n    echo second\n",
        )
        .write("justfile", "build:\n    FOO=root\n    echo root\n")
        .arg("api/")
        .stdout("first\n");
}

#[test]
fn the_recipe_runs_with_the_folder_as_working_directory() {
    Test::new()
        .write("api/justfile", "where:\n    FOO=1\n    basename $PWD\n")
        .write("justfile", "build:\n    FOO=root\n    echo root\n")
        .arg("api/where")
        .stdout("api\n");
}

#[test]
fn relative_paths_resolve_against_the_folder() {
    Test::new()
        .write("api/justfile", "read:\n    FOO=1\n    cat data.txt\n")
        .write("api/data.txt", "from api\n")
        .write("justfile", "build:\n    FOO=root\n    echo root\n")
        .arg("api/read")
        .stdout("from api\n");
}

#[test]
fn a_dotenv_file_resolves_against_the_folder() {
    Test::new()
        .write("api/justfile", "show:\n    FOO=1\n    echo $SECRET\n")
        .write("api/.env", "SECRET=from-api\n")
        .write("justfile", "build:\n    FOO=root\n    echo root\n")
        .arg("api/show")
        .stdout("from-api\n");
}

#[test]
fn listing_a_folders_recipes_works() {
    let stdout = Test::new()
        .write(
            "api/justfile",
            "# Build the API\nbuild:\n    FOO=1\n    echo build\n",
        )
        .write("justfile", "root:\n    FOO=1\n    echo root\n")
        .args(["-l", "api/"])
        .stdout_raw();

    assert!(stdout.contains("build"), "expected `build` in: {stdout}");
    assert!(
        !stdout.contains("root"),
        "should not list the root justfile's recipes: {stdout}",
    );
}

#[test]
fn arguments_pass_through_to_a_folder_scoped_recipe() {
    Test::new()
        .write("api/justfile", "greet NAME:\n    echo $NAME\n")
        .write("justfile", "build:\n    FOO=root\n    echo root\n")
        .args(["api/greet", "hello world"])
        .stdout("hello world\n");
}

#[test]
fn the_folder_path_does_not_walk_up_to_a_parent() {
    // `sub/` has no justfile of its own, and the search must not fall back to
    // the one in the parent directory.
    Test::new()
        .justfile(
            r#"
            build:
                FOO=root
                echo root
            "#,
        )
        .create_dir("sub")
        .arg("sub/build")
        .fails_with("");
}
