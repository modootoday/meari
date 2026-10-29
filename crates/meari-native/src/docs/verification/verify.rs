use super::super::{
    model::{Check, CheckStatus, Receipt},
    read_catalog, write_json,
};
use crate::{
    application,
    domain::{Viewport, widgets::Session},
    infrastructure::{artifacts, document::InteractiveDocument, tailwind::TailwindCompiler},
};
use meari::application::Language;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub fn verify(catalog_dir: &Path, executable: &Path, output: &Path) -> Result<(), String> {
    verify_selected(catalog_dir, executable, output, None)
}

pub(in super::super) fn verify_selected(
    catalog_dir: &Path,
    executable: &Path,
    output: &Path,
    selected: Option<&[usize]>,
) -> Result<(), String> {
    let catalog = read_catalog(catalog_dir)?;
    if let Some(ids) = selected {
        let unique: std::collections::BTreeSet<_> = ids.iter().copied().collect();
        if ids.is_empty()
            || unique.len() != ids.len()
            || ids.iter().any(|id| *id >= catalog.examples.len())
        {
            return Err("selection requires nonempty, unique, valid example IDs".into());
        }
    }
    let compiler = TailwindCompiler::pinned(executable)?;
    fs::create_dir(output).map_err(|e| e.to_string())?;
    write_json(
        &output.join("scope.json"),
        &serde_json::json!({"all_examples":selected.is_none(),"selected_ids":selected,"total_inventory":catalog.examples.len()}),
    )?;
    let mut receipts = vec![];
    let mut renderable = vec![];
    let mut classes = String::new();
    for example in &catalog.examples {
        let mut receipt = Receipt {
            id: example.id,
            status: example.collection.clone(),
            detail: String::new(),
            screenshot: None,
            interaction: "not-run".into(),
            accessibility: "not-run".into(),
            checks: super::super::completion::REQUIRED
                .iter()
                .map(|gate| Check {
                    gate: (*gate).into(),
                    status: CheckStatus::NotRun,
                    detail: "requires explicit execution/expected result".into(),
                })
                .collect(),
        };
        if selected.is_some_and(|ids| !ids.contains(&example.id)) {
            receipt.status = "not-run".into();
            receipt.detail = "outside explicit requested selection".into();
            receipts.push(receipt);
            continue;
        }
        if let Some(source) = &example.source {
            let code = fs::read_to_string(catalog_dir.join(source)).map_err(|e| e.to_string())?;
            let hash = format!("{:x}", Sha256::digest(code.as_bytes()));
            if example.sha256.as_ref() != Some(&hash) {
                return Err(format!("source hash mismatch: {}", example.id));
            }
            if !matches!(example.language.as_str(), "tsx" | "jsx") {
                receipt.status = "reference-only".into();
                receipt.detail =
                    "Installation/configuration/text snippet; not an executable component".into();
            } else {
                let original = meari::parse(&code, Language::Tsx).err();
                let wrapped = super::super::reference_adapter::adapt(&code)?;
                let adaptation = format!(
                    "Explicit typed/import/fragment adapter; upstream implementations are mapped to local recipes. Original module diagnostic: {}. ",
                    original
                        .map(|d| d.message)
                        .unwrap_or_else(|| "accepted".into())
                );
                match meari::parse(&wrapped, Language::Tsx) {
                    Err(error) => {
                        receipt.status = "frontend-blocked".into();
                        receipt.detail = adaptation.clone()
                            + &format!(
                                "{} [{}..{}]: {}",
                                error.code, error.span.start, error.span.end, error.message
                            );
                    }
                    Ok(component) => {
                        for language in [Language::Rust, Language::Tsx] {
                            let emitted =
                                meari::emit(&component, language).map_err(|e| e.message)?;
                            let parsed =
                                meari::parse(&emitted.code, language).map_err(|e| e.message)?;
                            if !component.semantic_eq(&parsed) {
                                return Err(format!(
                                    "semantic roundtrip mismatch for example {}",
                                    example.id
                                ));
                            }
                        }
                        set_check(
                            &mut receipt,
                            "roundtrip",
                            CheckStatus::Passed,
                            "both Rust and TSX semantic roundtrips asserted",
                        );
                        let prepared =
                            application::bind(&component, &BTreeMap::new()).and_then(|component| {
                                let initial =
                                    application::initial_state_projection(component.clone())?;
                                let session = Session::from_tree(&initial.root)?;
                                let markup = application::markup::build(&initial, &session)?;
                                Ok((component, markup.classes))
                            });
                        match prepared {
                            Err(error) => {
                                receipt.status = "native-blocked".into();
                                receipt.detail = adaptation.clone() + &error;
                            }
                            Ok((component, used)) => {
                                classes.push_str(&used);
                                classes.push(' ');
                                receipt.status = "pending-render".into();
                                receipt.detail = adaptation;
                                fs::write(
                                    output.join(format!("example-{}.adapter.tsx", example.id)),
                                    &wrapped,
                                )
                                .map_err(|e| e.to_string())?;
                                renderable.push((example.id, component));
                            }
                        }
                    }
                }
            }
        }
        receipts.push(receipt);
    }
    let theme = super::super::theme::reference();
    let css = compiler.compile(&classes, &theme)?;
    fs::write(output.join("compiled.css"), &css).map_err(|e| e.to_string())?;
    let total_variants = renderable.len() * 4;
    let mut completed_variants: usize = 0;
    for (id, component) in renderable {
        let receipt = &mut receipts[id];
        let result = (|| {
            let mut variants = Vec::new();
            for (variant, width, dark) in [
                ("wide-light", 960, false),
                ("narrow-light", 390, false),
                ("wide-dark", 960, true),
                ("narrow-dark", 390, true),
            ] {
                let state_dir = output.join(format!("example-{id}-{variant}"));
                fs::create_dir(&state_dir).map_err(|e| e.to_string())?;
                let mut document = InteractiveDocument::new(
                    component.clone(),
                    &css,
                    Viewport {
                        width,
                        height: 720,
                        dark,
                    },
                )?;
                let initial = artifacts::capture(&mut document, &state_dir, "initial")?;
                if receipt.screenshot.is_none() {
                    receipt.screenshot = Some(format!("example-{id}-{variant}/initial.png"));
                }
                set_check(
                    receipt,
                    "native-render",
                    CheckStatus::Passed,
                    "native raster/layout/AccessKit tree captured; upstream visual parity not implied",
                );
                receipt.accessibility = "tree-captured; OS/screen-reader acceptance not-run".into();
                let exercised =
                    super::super::interaction::verify_recorded(&mut document, Some(&state_dir))?;
                let (buttons, inputs) =
                    super::super::controls::verify(&mut document, Some(&state_dir))?;
                receipt.interaction = format!(
                    "{exercised} native contract assertions (state/ARIA/focus/navigation); {buttons} button/toggle controls asserted (see controls.json); {inputs} text/file controls asserted; OS keyboard/pointer/upstream controlled callbacks not verified"
                );
                set_check(
                    receipt,
                    "native-interaction",
                    CheckStatus::Passed,
                    &receipt.interaction.clone(),
                );
                let final_state = artifacts::capture(&mut document, &state_dir, "final")?;
                variants.push(serde_json::json!({"variant":variant,"initial":initial,"final":final_state,"native_assertions":exercised,"buttons":buttons,"inputs":inputs}));
                write_json(
                    &output.join(format!("example-{id}.variants.json")),
                    &variants,
                )?;
                completed_variants += 1;
                write_json(
                    &output.join("progress.json"),
                    &serde_json::json!({"completed_variants":completed_variants,"planned_variants":total_variants,"last_example":id,"last_variant":variant}),
                )?;
                if completed_variants.is_multiple_of(25) {
                    println!("native variants: {completed_variants}/{total_variants}");
                }
            }
            Ok::<_, String>(())
        })();
        match result {
            Ok(()) => receipt.status = "rendered-partial-validation".into(),
            Err(error) => {
                receipt.status = "execution-failed".into();
                receipt.detail.push_str(&format!("; {error}"));
                set_check(receipt, "native-interaction", CheckStatus::Failed, &error);
            }
        }
    }
    write_json(&output.join("receipts.json"), &receipts)?;
    let completion = super::super::completion::evaluate(&catalog, &receipts);
    write_json(&output.join("completion.json"), &completion)?;
    let counts = receipts
        .iter()
        .fold(BTreeMap::<String, usize>::new(), |mut counts, row| {
            *counts.entry(row.status.clone()).or_default() += 1;
            counts
        });
    write_json(
        &output.join("summary.json"),
        &serde_json::json!({"examples":receipts.len(),"counts":counts,"full_acceptance":completion.full_acceptance,"completed_variants":completed_variants,"planned_variants":total_variants,"viewport_widths":[960,390],"viewport_height":720,"color_schemes":["light","dark"]}),
    )?;
    println!(
        "{}",
        serde_json::to_string(&counts).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn set_check(receipt: &mut Receipt, gate: &str, status: CheckStatus, detail: &str) {
    if let Some(check) = receipt.checks.iter_mut().find(|check| check.gate == gate) {
        check.status = status;
        check.detail = detail.into();
    }
}
