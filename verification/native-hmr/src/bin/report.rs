use native_hmr_verification::{domain::Report, infrastructure::files};

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() == 1, "hmr-report REPORT.json");
    let report: Report = files::read_json(std::path::Path::new(&args[0]))?;
    println!("{}", serde_json::to_string_pretty(&report.summary())?);
    Ok(())
}
