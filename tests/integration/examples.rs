use super::*;

#[test]
fn examples() {
  for result in fs::read_dir(format!("{V1_ROOT}/examples")).unwrap() {
    let entry = result.unwrap();
    let path = entry.path();

    println!("Parsing `{}`…", path.display());

    let output = Command::new(JUST)
      .arg("--justfile")
      .arg(&path)
      .arg("--dump")
      .output()
      .unwrap();

    assert_success(&output);
  }
}
