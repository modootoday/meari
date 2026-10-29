pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

pub fn run_verification(root: std::path::PathBuf, position: [i32; 2]) -> anyhow::Result<()> {
    let mut session = infrastructure::session::Session::start(root, position)?;
    let report = application::verify(&mut session)?;
    let output = session.root.join("report.json");
    infrastructure::files::write_json(&output, &report)?;
    println!(
        "PASS: {} Rust code swaps; report={}",
        report.compatible_code_swaps,
        output.display()
    );
    Ok(())
}
