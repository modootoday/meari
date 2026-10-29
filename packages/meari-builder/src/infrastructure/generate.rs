use meari::{
    application::{
        app_bundle::ApplicationDefinition, module_app_bundle::ModuleApplicationDefinition,
    },
    domain::build::{BuildPath, TargetKind},
};
use std::{collections::BTreeMap, path::Path};
mod application;
mod assets;

pub(super) fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().expect("static bundle path")
}
fn dependency_path(path: &Path) -> Result<String, String> {
    let path = path
        .to_str()
        .ok_or("framework path is not UTF-8")?
        .replace('\\', "/");
    // Cargo treats //?/ as a URL, unlike Win32 filesystem APIs.
    let path = if let Some(unc) = path.strip_prefix("//?/UNC/") {
        format!("//{unc}")
    } else {
        path.strip_prefix("//?/").unwrap_or(&path).to_owned()
    };
    serde_json::to_string(&path).map_err(|error| error.to_string())
}
pub(super) fn project(
    framework: &Path,
    definition: &ApplicationDefinition,
    modules: Option<&ModuleApplicationDefinition>,
    css: &str,
    resources: Option<&meari::application::assets::VerifiedAssets>,
) -> Result<(String, BTreeMap<BuildPath, Vec<u8>>), String> {
    let name = format!("meari_app_{}", definition.stamp.graph_hash.as_str());
    let core = dependency_path(&framework.join("packages/meari"))?;
    let native = dependency_path(&framework.join("packages/meari-native"))?;
    let paint = dependency_path(&framework.join("packages/meari-paint"))?;
    let dependencies = match definition.stamp.target.kind {
        TargetKind::Native => format!(
            "meari = {{ path = {core} }}\nmeari-native = {{ path = {native}, features = [\"window\"] }}\nserde_json = \"1\"\n"
        ),
        TargetKind::Web => {
            format!("meari = {{ path = {core}, features = [\"wasm\"] }}\nserde_json = \"1\"\n")
        }
    };
    let patches = if definition.stamp.target.kind == TargetKind::Native {
        format!("\n[patch.crates-io]\nblitz-paint = {{ path = {paint} }}\n")
    } else {
        String::new()
    };
    let cargo = format!(
        "[workspace]\n[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = \"2024\"\n[dependencies]\n{dependencies}{patches}"
    );
    let mut files = BTreeMap::from([
        (path("Cargo.toml"), cargo.into_bytes()),
        (
            path("app.json"),
            serde_json::to_vec(definition).map_err(|error| error.to_string())?,
        ),
        (path("styles.css"), css.as_bytes().to_vec()),
        (
            path("src/application.rs"),
            application::source(definition.stamp.target.kind, modules.is_some())
                .as_bytes()
                .to_vec(),
        ),
        (
            path("src/main.rs"),
            match definition.stamp.target.kind {
                TargetKind::Native => NATIVE,
                TargetKind::Web => WEB,
            }
            .as_bytes()
            .to_vec(),
        ),
    ]);
    if let Some(modules) = modules {
        modules.validate()?;
        if serde_json::to_value(&modules.application).map_err(|error| error.to_string())?
            != serde_json::to_value(definition).map_err(|error| error.to_string())?
        {
            return Err("generated module application differs from its app definition".into());
        }
        files.insert(
            path("module-app.json"),
            serde_json::to_vec(modules).map_err(|error| error.to_string())?,
        );
    }
    files.extend(assets::files(resources, definition.stamp.target.kind)?);
    if definition.stamp.target.kind == TargetKind::Web {
        let copy = if resources.is_some() {
            "<link data-trunk rel=\"copy-dir\" href=\"assets\" data-target-path=\"assets\">"
        } else {
            ""
        };
        files.insert(path("index.html"),format!("<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><link data-trunk rel=\"rust\" data-bin=\"{name}\">{copy}</head><body></body></html>").into_bytes());
    }
    Ok((name, files))
}

const NATIVE: &str = r#"use meari::domain::build::ContentHash;
use meari_native::{application::{Renderer,WindowOptions},domain::widgets::Action,infrastructure::window};
mod application;
mod assets;
fn run()->Result<(),String> {
    let (definition,mut document)=application::mount()?;
    let mut snapshot=false;let mut seconds=None;let mut actions=Vec::new();let mut args=std::env::args().skip(1);
    while let Some(argument)=args.next() {match argument.as_str() {
        "--snapshot"=>snapshot=true,
        "--action"=>actions.push(args.next().ok_or("--action requires a source node path")?),
        "--show-seconds"=>seconds=Some(args.next().ok_or("--show-seconds requires a duration")?.parse::<u64>().map_err(|error|error.to_string())?),
        _=>return Err(format!("unknown application argument: {argument}")),
    }}
    let mut receipts=Vec::new();for path in actions {let changed=document.dispatch(Action::Activate{path:path.clone()})?;receipts.push(serde_json::json!({"path":path,"changed":changed}));}
    if snapshot {
        let viewport=document.viewport.clone();document.renderer.layout(&viewport,0.0)?;let pixels=document.renderer.rasterize(&viewport)?;
        let mut semantic_nodes=meari_native::infrastructure::accessibility::tree(&document).nodes;semantic_nodes.sort_by_key(|(id,_)|id.0);
        let accessibility=semantic_nodes.into_iter().map(|(id,node)|serde_json::json!({"id":id,"role":format!("{:?}",node.role()),"label":node.label(),"disabled":node.is_disabled()})).collect::<Vec<_>>();
        let modules=if application::MODULES {Some(&document.module_projection()?.instances)} else {None};
        let state=if application::MODULES {None} else {Some(document.component_state()?)};
        println!("{}",serde_json::json!({"stamp":definition.stamp,"state":state,"module_states":modules,"actions":receipts,"frame_sha256":ContentHash::of(&pixels),"rgba_bytes":pixels.len(),"viewport":viewport,"accessibility":accessibility,"assets":document.renderer.asset_report()?}));Ok(())
    } else {window::show_with_options(document,seconds,WindowOptions{title:definition.title})}
}
fn main()->std::process::ExitCode {match run(){Ok(())=>std::process::ExitCode::SUCCESS,Err(error)=>{eprintln!("{}",serde_json::json!({"error":error}));std::process::ExitCode::FAILURE}}}
"#;

const WEB: &str = r#"use meari::infrastructure::web_document::WebDocument;
use std::cell::RefCell;
mod application;
mod assets;
thread_local! {static APP:RefCell<Option<WebDocument>>=const{RefCell::new(None)};}
fn run()->Result<(),String> {
    let mounted=application::mount()?;
    APP.with_borrow_mut(|slot|*slot=Some(mounted));Ok(())
}
fn main() {run().expect("Meari application startup");}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cargo_dependency_paths_strip_win32_verbatim_prefix() {
        assert_eq!(
            dependency_path(Path::new(r"\\?\D:\source with spaces\meari")).unwrap(),
            r#""D:/source with spaces/meari""#
        );
        assert_eq!(
            dependency_path(Path::new(r"\\?\UNC\server\share\meari")).unwrap(),
            r#""//server/share/meari""#
        );
        assert_eq!(
            dependency_path(Path::new("/source/meari")).unwrap(),
            r#""/source/meari""#
        );
    }
}
