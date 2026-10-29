fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() == 3, "hmr-host SESSION X Y");
    native_hmr_verification::presentation::run_host(
        (&args[0]).into(),
        [args[1].parse()?, args[2].parse()?],
    )
}
