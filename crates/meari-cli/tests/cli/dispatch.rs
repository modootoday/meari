use std::process::Command;

fn meari(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_meari"))
        .args(args)
        .output()
        .expect("run meari")
}

#[test]
fn help_lists_every_command_group() {
    let out = meari(&["--help"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for group in ["convert", "native", "build"] {
        assert!(text.contains(group), "{group} missing from help");
    }
}

#[test]
fn groups_delegate_to_legacy_help() {
    for (group, marker) in [
        ("convert", "meari-cli"),
        ("native", "meari-native"),
        ("build", "meari-builder"),
    ] {
        let out = meari(&[group, "--help"]);
        assert!(out.status.success(), "{group} --help failed");
        assert!(String::from_utf8_lossy(&out.stdout).contains(marker));
    }
}

#[test]
fn unknown_command_exits_with_usage_error() {
    assert_eq!(meari(&["nope"]).status.code(), Some(2));
}
