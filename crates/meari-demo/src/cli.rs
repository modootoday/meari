//! Command-line options of the demo controller.
use std::path::PathBuf;

pub const USAGE: &str = "usage: cargo xtask demo [FIXTURE|INDEX] [--list] [--seconds N] [--out DIR] [--chrome PATH] [--tailwind PATH]\n  interactive keys (type + Enter): n next, p previous, l list, <index|name> jump, r reload, q quit";

pub struct Options {
    pub start: Option<String>,
    pub list_only: bool,
    pub seconds: Option<u64>,
    pub out: PathBuf,
    pub chrome: String,
    pub tailwind: PathBuf,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Options {
            start: None,
            list_only: false,
            seconds: None,
            out: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/demo")),
            chrome: meari_visual::host::default_chrome(),
            tailwind: meari_visual::host::default_tailwind(&PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../.."
            ))),
        };
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            let mut value =
                |flag: &str| iter.next().cloned().ok_or(format!("{flag} needs a value"));
            match arg.as_str() {
                "--help" | "-h" => return Err(USAGE.into()),
                "--list" => options.list_only = true,
                "--seconds" => {
                    let raw = value("--seconds")?;
                    options.seconds =
                        Some(raw.parse().map_err(|_| format!("bad --seconds {raw}"))?);
                }
                "--out" => options.out = value("--out")?.into(),
                "--chrome" => options.chrome = value("--chrome")?,
                "--tailwind" => options.tailwind = value("--tailwind")?.into(),
                flag if flag.starts_with("--") => {
                    return Err(format!("unknown flag {flag}\n{USAGE}"));
                }
                name => options.start = Some(name.to_owned()),
            }
        }
        Ok(options)
    }
}
