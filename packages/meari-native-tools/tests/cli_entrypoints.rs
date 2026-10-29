use std::process::Command;

fn help(executable: &str) -> String {
    let output = Command::new(executable).arg("--help").output().unwrap();
    assert!(
        output.status.success(),
        "help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn native_executable_keeps_its_name_and_existing_command_surface() {
    let output = help(env!("CARGO_BIN_EXE_meari-native"));
    for command in [
        "meari-native verify",
        "meari-native show",
        "meari-native styles",
    ] {
        assert!(output.contains(command), "missing command: {command}");
    }
}

#[test]
fn kitchen_sink_executable_keeps_batch_verification_commands() {
    let output = help(env!("CARGO_BIN_EXE_meari-kitchen-sink"));
    for command in [
        "meari-kitchen-sink library-check",
        "library-check-gl",
        "run[-gl]",
    ] {
        assert!(output.contains(command), "missing command: {command}");
    }
}
