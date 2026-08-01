use super::*;

#[test]
fn print_changelog() {
  Test::new()
    .justfile("")
    .args(["--changelog"])
    .stdout(fs::read_to_string(format!("{V1_ROOT}/CHANGELOG.md")).unwrap())
    .success();
}
