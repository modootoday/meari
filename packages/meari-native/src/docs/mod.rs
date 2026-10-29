//! Pinned example inventory and headless native kitchen-sink execution.
mod blockers;
mod builtin;
mod collect;
mod completion;
mod controls;
#[cfg(all(feature = "gl", windows))]
mod gl_suite;
mod interaction;
mod model;
mod reference_adapter;
mod screen_suite;
mod source_state;
mod styles_suite;
mod theme;
mod verify;

use std::{env, fs, path::Path};

pub fn run() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("library-check") if args.len()==3 => builtin::run(Path::new(&args[1]),Path::new(&args[2])),
        Some("library-check-gl") if args.len()==3 => {
            #[cfg(all(feature="gl",windows))]
            { let _scope=crate::infrastructure::gl::GlScope::enter()?; builtin::run(Path::new(&args[1]),Path::new(&args[2])) }
            #[cfg(not(all(feature="gl",windows)))]
            { Err("library-check-gl requires Windows and --features gl".into()) }
        },
        Some("--help" | "help") => {
            println!("meari-kitchen-sink library-check TAILWIND_EXE NEW_OUTPUT_DIRECTORY\nmeari-kitchen-sink library-check-gl TAILWIND_EXE NEW_OUTPUT_DIRECTORY\nrun[-gl] CATALOG_DIRECTORY TAILWIND_EXE NEW_OUTPUT_DIRECTORY executes the complete catalog and acceptance gates. library-check executes owned stories only; it does not prove upstream coverage.");
            Ok(())
        },
        Some("screen-check") if args.len()==2 => screen_suite::verify(Path::new(&args[1])),
        Some("screen-check-gl") if args.len()==2 => {
            #[cfg(all(feature="gl",windows))]
            { let _scope=crate::infrastructure::gl::GlScope::enter()?; screen_suite::verify(Path::new(&args[1])) }
            #[cfg(not(all(feature="gl",windows)))]
            { Err("screen-check-gl requires Windows and --features gl".into()) }
        },
        Some("gl-check") if args.len()==2 => {
            #[cfg(all(feature="gl",windows))]
            { gl_suite::verify(Path::new(&args[1])) }
            #[cfg(not(all(feature="gl",windows)))]
            { Err("gl-check requires Windows and --features gl; no CPU fallback".into()) }
        },
        Some("run-gl") if args.len()==4 => {
            #[cfg(all(feature="gl",windows))]
            {
                let windows_before=crate::infrastructure::gl::owned_window_count();
                let scope=crate::infrastructure::gl::GlScope::enter()?;
                let result=run_suite(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[3]));
                let lifecycle=gl_suite::verify(&Path::new(&args[3]).join("gl-lifecycle.json"));
                drop(scope);
                let windows_after=crate::infrastructure::gl::owned_window_count();
                write_json(&Path::new(&args[3]).join("gl-summary.json"),&serde_json::json!({"backend":"skia-opengl-vector","suite_error":result.as_ref().err(),"lifecycle_passed":lifecycle.is_ok(),"lifecycle_error":lifecycle.as_ref().err(),"owned_windows_before":windows_before,"owned_windows_after":windows_after}))?;
                lifecycle?; if windows_after!=windows_before {return Err("GL host scope leaked an owned window".into());} result
            }
            #[cfg(not(all(feature="gl",windows)))]
            { Err("run-gl requires Windows and --features gl; no CPU fallback".into()) }
        },
        Some("blockers") if args.len()==3 => blockers::report(Path::new(&args[1]),Path::new(&args[2])),
        Some("run") if args.len() == 4 => run_suite(
            Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3]),
        ),
        Some("collect") if args.len() == 4 => collect::collect(
            Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3]),
        ),
        Some("verify") if args.len() == 4 => verify::verify(
            Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3]),
        ),
        Some("verify") if args.len() > 4 => {
            let ids:Result<Vec<usize>,_>=args[4..].iter().map(|id|id.parse()).collect();
            verify::verify_selected(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[3]),Some(&ids.map_err(|e|format!("example ID: {e}"))?))
        },
        Some("accept") if args.len()==3 => {
            let catalog=read_catalog(Path::new(&args[1]))?;
            let receipts:Vec<model::Receipt>=serde_json::from_slice(&fs::read(Path::new(&args[2]).join("receipts.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            let result=completion::evaluate(&catalog,&receipts);
            println!("{}",serde_json::to_string_pretty(&result).map_err(|e|e.to_string())?);
            if result.full_acceptance {Ok(())} else {Err("all-feature acceptance is incomplete; inspect missing gates/inventory gaps".into())}
        }
        _ => Err("meari-kitchen-sink run CATALOG_DIRECTORY TAILWIND_EXE NEW_OUTPUT_DIRECTORY\nmeari-kitchen-sink run-gl CATALOG_DIRECTORY TAILWIND_EXE NEW_OUTPUT_DIRECTORY (Windows, --features gl)\nmeari-kitchen-sink gl-check NEW_REPORT.json (Windows, --features gl)\nmeari-kitchen-sink collect SHADCN_CHECKOUT TAILWIND_DOCS_CHECKOUT NEW_DIRECTORY\nmeari-kitchen-sink verify CATALOG_DIRECTORY TAILWIND_EXE NEW_RECEIPTS_DIRECTORY [EXAMPLE_ID ...]\nmeari-kitchen-sink accept CATALOG_DIRECTORY RECEIPTS_DIRECTORY\nmeari-kitchen-sink blockers RECEIPTS.json NEW_REPORT.json".into()),
    }
}

/// Runs the native renderer against every inventory row, then enforces acceptance.
/// Artifacts are written before an incomplete suite returns an error.
pub fn run_suite(catalog: &Path, compiler: &Path, output: &Path) -> Result<(), String> {
    verify::verify(catalog, compiler, output)?;
    let widgets = builtin::run(compiler, &output.join("library-suite"));
    let styles = styles_suite::run(compiler, &output.join("styles-suite"));
    let screen = screen_suite::verify(&output.join("screen-media.json"));
    let inventory = read_catalog(catalog)?;
    let receipts: Vec<model::Receipt> =
        serde_json::from_slice(&fs::read(output.join("receipts.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let completion = completion::evaluate(&inventory, &receipts);
    let passed = completion.full_acceptance && widgets.is_ok() && styles.is_ok() && screen.is_ok();
    write_json(
        &output.join("run.json"),
        &serde_json::json!({
            "execution":"headless-native-cli", "web_server_required":false,
            "library_widgets":{"passed":widgets.is_ok(),"error":widgets.err()},
            "custom_styles":{"passed":styles.is_ok(),"error":styles.err()},
            "screen_media":{"passed":screen.is_ok(),"error":screen.err()},
            "official_inventory":completion,
        }),
    )?;
    if passed {
        Ok(())
    } else {
        Err(format!(
            "native kitchen-sink incomplete; see {}",
            output.join("completion.json").display()
        ))
    }
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn read_catalog(directory: &Path) -> Result<model::Catalog, String> {
    let catalog: model::Catalog = serde_json::from_slice(
        &fs::read(directory.join("catalog.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if catalog.schema != 1 || catalog.sources.len() != 2 {
        return Err("unsupported catalog schema".into());
    }
    for (id, example) in catalog.examples.iter().enumerate() {
        if example.id != id || example.page >= catalog.pages.len() {
            return Err("invalid example identity/page".into());
        }
        if let Some(source) = &example.source
            && (source.contains('\\')
                || Path::new(source)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_))))
        {
            return Err("unsafe catalog source path".into());
        }
    }
    Ok(catalog)
}
