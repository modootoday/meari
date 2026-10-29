use crate::{
    application::{self, StylesheetRenderRequest},
    infrastructure::{
        artifacts, document::InteractiveDocument, stylesheet::DirectCssCompiler,
        tailwind::TailwindCompiler,
    },
};
use meari::domain::design::stylesheet::StylesheetMode;
use std::{fs, path::Path};

pub fn execute(args: &[String]) -> Result<(), String> {
    if !(3..=4).contains(&args.len()) {
        return Err("meari-native styles REQUEST.json NEW_OUTPUT_DIRECTORY [TAILWIND_EXE]".into());
    }
    let request: StylesheetRenderRequest =
        serde_json::from_slice(&fs::read(&args[1]).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let (component, compiled) = match request.stylesheet.mode {
        StylesheetMode::Css => application::prepare_stylesheet(&request, &DirectCssCompiler)?,
        StylesheetMode::Tailwind => {
            let executable = args
                .get(3)
                .ok_or("Tailwind mode requires pinned TAILWIND_EXE")?;
            let compiler = TailwindCompiler::pinned(Path::new(executable))?;
            application::prepare_stylesheet(&request, &compiler)?
        }
    };
    let output = Path::new(&args[2]);
    fs::create_dir(output).map_err(|e| e.to_string())?;
    fs::write(output.join("compiled.css"), &compiled.css).map_err(|e| e.to_string())?;
    artifacts::json(&output.join("stylesheet.json"), &compiled)?;
    let mut document =
        InteractiveDocument::new(component, &compiled.css, request.viewport.clone())?;
    if let Some(element) = &request.screen_element {
        document.resize_screen(&request.viewport, element)?;
    }
    let render = artifacts::capture(&mut document, output, "initial")?;
    artifacts::json(&output.join("render.json"), &render)?;
    println!(
        "{}",
        serde_json::json!({"compiler":compiled.compiler,"source_files":compiled.source_sha256.len(),"css_sha256":compiled.css_sha256,"native_full_acceptance":false})
    );
    Ok(())
}
