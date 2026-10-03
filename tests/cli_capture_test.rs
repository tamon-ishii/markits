use std::process::Command;

#[test]
fn test_cli_capture_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("capture")
        .arg("--help")
        .output()
        .expect("Failed to execute markits capture --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--detect-ui"), "Help should describe --detect-ui");
    assert!(stdout.contains("--uimap"), "Help should describe --uimap");
    assert!(stdout.contains("--target"), "Help should describe --target");
    assert!(stdout.contains("--mark"), "Help should describe --mark");
}
