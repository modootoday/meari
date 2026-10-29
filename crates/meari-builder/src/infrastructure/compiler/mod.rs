use super::{
    files::{framework_hash, hash_file},
    generate,
    tools::{RegisteredTool, rustc_metadata},
};
use crate::application::project::{self, AppConfiguration};
use meari::{
    application::builder::*,
    domain::{build::*, design::stylesheet::StylesheetMode},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
mod assets;
mod components;
mod process;

pub struct CompilerRegistration {
    pub framework_root: PathBuf,
    pub work_root: PathBuf,
    pub target_dir: PathBuf,
    pub configuration: BuildPath,
    pub cargo: RegisteredTool,
    pub rustc: RegisteredTool,
    pub trunk: Option<RegisteredTool>,
    pub wasm_bindgen: Option<RegisteredTool>,
    pub tailwind: Option<PathBuf>,
}
pub struct RustAppCompiler {
    registration: CompilerRegistration,
    framework: ContentHash,
    builder: ContentHash,
    release: String,
    commit: String,
    host: String,
}
impl RustAppCompiler {
    pub fn new(mut registration: CompilerRegistration) -> Result<Self, String> {
        registration.framework_root =
            fs::canonicalize(&registration.framework_root).map_err(|error| error.to_string())?;
        fs::create_dir_all(&registration.work_root).map_err(|error| error.to_string())?;
        fs::create_dir_all(&registration.target_dir).map_err(|error| error.to_string())?;
        registration.work_root =
            fs::canonicalize(&registration.work_root).map_err(|error| error.to_string())?;
        registration.target_dir =
            fs::canonicalize(&registration.target_dir).map_err(|error| error.to_string())?;
        if !registration.cargo.pin.version.starts_with("cargo ")
            || !registration.rustc.pin.version.starts_with("rustc ")
        {
            return Err("Cargo/Rustc tool registration kind mismatch".into());
        }
        let framework = framework_hash(&registration.framework_root)?;
        let builder = hash_file(&std::env::current_exe().map_err(|error| error.to_string())?)?;
        let (release, commit, host) = rustc_metadata(&registration.rustc)?;
        Ok(Self {
            registration,
            framework,
            builder,
            release,
            commit,
            host,
        })
    }
    pub fn specification(
        &self,
        target: &BuildTarget,
        configuration: &AppConfiguration,
    ) -> Result<TargetBuildSpec, String> {
        target.validate()?;
        let mut providers = BTreeMap::from([
            ("cargo".into(), self.registration.cargo.pin.clone()),
            ("rustc".into(), self.registration.rustc.pin.clone()),
            (
                "meari-framework".into(),
                ProviderPin {
                    version: "source-snapshot".into(),
                    revision: self.framework.as_str().into(),
                    content: self.framework.clone(),
                },
            ),
            (
                "builder-host".into(),
                ProviderPin {
                    version: env!("CARGO_PKG_VERSION").into(),
                    revision: "rust-app-v3-declared-assets".into(),
                    content: self.builder.clone(),
                },
            ),
        ]);
        let mut capabilities = BTreeSet::from([
            "css".into(),
            "component-state".into(),
            "component-modules".into(),
            "declared-assets".into(),
        ]);
        match target.kind {
            TargetKind::Native => {
                if target.triple != self.host {
                    return Err("native cross compiler is not registered".into());
                }
                capabilities.extend(["native-render".into(), "native-window".into()]);
            }
            TargetKind::Web => {
                let trunk = self
                    .registration
                    .trunk
                    .as_ref()
                    .ok_or("registered Trunk is required for Web app bundling")?;
                let bindgen = self
                    .registration
                    .wasm_bindgen
                    .as_ref()
                    .ok_or("registered wasm-bindgen is required for Web app bundling")?;
                trunk.verify()?;
                bindgen.verify()?;
                providers.insert("trunk".into(), trunk.pin.clone());
                providers.insert("wasm-bindgen".into(), bindgen.pin.clone());
                capabilities.insert("web-dom".into());
            }
        }
        if configuration.stylesheet.mode == StylesheetMode::Tailwind {
            let executable = self
                .registration
                .tailwind
                .as_ref()
                .ok_or("pinned Tailwind compiler is required")?;
            meari_native::infrastructure::tailwind::TailwindCompiler::windows_x64(executable)?;
            let content = hash_file(executable)?;
            providers.insert(
                "tailwindcss".into(),
                ProviderPin {
                    version: meari_native::infrastructure::tailwind::VERSION.into(),
                    revision: content.as_str().into(),
                    content,
                },
            );
            capabilities.insert("tailwind-css".into());
        }
        Ok(TargetBuildSpec {
            available_capabilities: capabilities,
            toolchain: BuildToolchain {
                rustc_release: self.release.clone(),
                rustc_commit: self.commit.clone(),
                cargo_release: self.registration.cargo.pin.version.clone(),
                providers,
                settings: BTreeMap::from([
                    ("profile".into(), "debug".into()),
                    ("offline".into(), "true".into()),
                    ("jobs".into(), "1".into()),
                    (
                        "configuration".into(),
                        self.registration.configuration.as_str().into(),
                    ),
                ]),
            },
        })
    }
    fn verify(&self) -> Result<(), String> {
        self.registration.cargo.verify()?;
        self.registration.rustc.verify()?;
        if framework_hash(&self.registration.framework_root)? != self.framework {
            return Err("framework source changed after compiler registration".into());
        }
        Ok(())
    }
}
impl TargetCompiler for RustAppCompiler {
    fn compile(
        &self,
        task: &BuildTask,
        inputs: &BTreeMap<BuildPath, Vec<u8>>,
    ) -> Result<BuildBundle, BuildFailure> {
        self.verify()?;
        let config = project::configuration(&self.registration.configuration, inputs)?;
        let expected = self.specification(&task.target, &config)?;
        if expected.toolchain != task.toolchain
            || expected.available_capabilities != task.available_capabilities
        {
            return Err("app compiler pins/settings differ from the requested task".into());
        }
        for (path, input) in &task.graph.files {
            if !matches!(
                input.role,
                InputRole::Component
                    | InputRole::Stylesheet
                    | InputRole::Runtime
                    | InputRole::Icon
                    | InputRole::Font
                    | InputRole::Image
            ) || (input.role == InputRole::Runtime && *path != self.registration.configuration)
            {
                return Err(BuildFailure::file(
                    path.clone(),
                    "app asset adapter is not registered for this input".into(),
                ));
            }
        }
        let mut components = components::prepare(task, inputs, &config)?;
        let stylesheet = project::stylesheet(&task.graph, inputs, &config.stylesheet)?;
        let resources = assets::prepare(&task.graph, inputs, stylesheet)?;
        let empty_assets = meari::domain::assets::AssetManifest {
            schema_version: 1,
            files: BTreeMap::new(),
        };
        let markup_references = assets::rewrite_components(
            &mut components,
            &task.graph.entry,
            resources
                .verified
                .as_ref()
                .map_or(&empty_assets, |assets| assets.manifest()),
        )?;
        let definition = &components.definition;
        let bound = &components.projected;
        let stylesheet = &resources.project;
        let compiled = match stylesheet.mode {
            StylesheetMode::Css => meari_native::application::stylesheet_project(
                bound,
                stylesheet,
                &config.theme,
                &meari_native::infrastructure::stylesheet::DirectCssCompiler,
            ),
            StylesheetMode::Tailwind => meari_native::application::stylesheet_project(
                bound,
                stylesheet,
                &config.theme,
                &meari_native::infrastructure::tailwind::TailwindCompiler::windows_x64(
                    self.registration
                        .tailwind
                        .as_ref()
                        .ok_or("Tailwind registration missing")?,
                )?,
            ),
        }
        .map_err(|message| BuildFailure::file(config.stylesheet.entry.clone(), message))?;
        if task.target.kind == TargetKind::Web {
            components.validate_web()?;
        } else if let Some(assets) = resources.verified.clone() {
            if let Some(modules) = &components.modules {
                meari_native::infrastructure::document::InteractiveDocument::from_module_application_with_assets(modules, &compiled.css, assets)?;
            } else {
                meari_native::infrastructure::document::InteractiveDocument::from_application_with_assets(definition, &compiled.css, assets)?;
            }
        } else if let Some(modules) = &components.modules {
            meari_native::infrastructure::document::InteractiveDocument::from_module_application(
                modules,
                &compiled.css,
            )?;
        } else {
            meari_native::infrastructure::document::InteractiveDocument::from_application(
                definition,
                &compiled.css,
            )?;
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "compiler attempt sequence exhausted")?;
        let parent = self
            .registration
            .work_root
            .join(task.ticket.session.as_str())
            .join(task.ticket.generation.to_string());
        fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let directory = parent.join(format!(
            "{:?}-{}-{sequence}",
            task.target.kind,
            std::process::id()
        ));
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let (name, mut files) = generate::project(
            &self.registration.framework_root,
            definition,
            components.modules.as_ref(),
            &compiled.css,
            resources.verified.as_ref(),
        )?;
        for (path, bytes) in &files {
            let destination = directory.join(path.as_str());
            fs::create_dir_all(destination.parent().ok_or("generated path has no parent")?)
                .map_err(|error| error.to_string())?;
            fs::write(destination, bytes).map_err(|error| error.to_string())?;
        }
        fs::copy(
            self.registration.framework_root.join("Cargo.lock"),
            directory.join("Cargo.lock"),
        )
        .map_err(|error| error.to_string())?;
        let products = match task.target.kind {
            TargetKind::Native => {
                process::native(&self.registration, &directory, &name, &task.target.triple)?
            }
            TargetKind::Web => process::web(&self.registration, &directory)?,
        };
        self.verify()?;
        files.extend(components.files);
        files.extend(resources.originals);
        files.insert(
            generate::path("build/markup-assets.json"),
            serde_json::to_vec(&markup_references).map_err(|error| error.to_string())?,
        );
        if let Some(assets) = &resources.verified {
            files.insert(
                generate::path("runtime/asset-manifest.json"),
                serde_json::to_vec(assets.manifest()).map_err(|error| error.to_string())?,
            );
        }
        files.insert(
            generate::path("build/css-assets.json"),
            serde_json::to_vec(&resources.references).map_err(|error| error.to_string())?,
        );
        files.insert(
            generate::path("runtime/application.json"),
            serde_json::to_vec(definition).map_err(|error| error.to_string())?,
        );
        if let Some(modules) = &components.modules {
            files.insert(
                generate::path("runtime/module-application.json"),
                serde_json::to_vec(modules).map_err(|error| error.to_string())?,
            );
        }
        files.insert(
            generate::path("build/stylesheet-receipt.json"),
            serde_json::to_vec(&compiled).map_err(|error| error.to_string())?,
        );
        files.insert(
            generate::path("build/Cargo.lock"),
            fs::read(directory.join("Cargo.lock")).map_err(|error| error.to_string())?,
        );
        let mut roles: BTreeMap<_, _> = files
            .keys()
            .map(|path| {
                (
                    path.clone(),
                    if path.as_str().ends_with(".rs")
                        || path.as_str().starts_with("source/original/")
                        || path.as_str().starts_with("source/styles/")
                    {
                        OutputRole::GeneratedSource
                    } else if path.as_str() == "styles.css" {
                        OutputRole::Stylesheet
                    } else {
                        OutputRole::Asset
                    },
                )
            })
            .collect();
        for (path, (role, bytes)) in products {
            if files.insert(path.clone(), bytes).is_some() {
                return Err("compiler output collides with a generated source asset".into());
            }
            roles.insert(path, role);
        }
        let outputs = files
            .iter()
            .map(|(path, bytes)| {
                (
                    path.clone(),
                    OutputFile {
                        role: roles[path],
                        content: ContentHash::of(bytes),
                        length: bytes.len() as u64,
                    },
                )
            })
            .collect();
        let mut source_maps = components.mappings;
        source_maps.extend(resources.mappings);
        let mut required = BTreeSet::from(["css".into()]);
        if resources.verified.is_some() {
            required.insert("declared-assets".into());
        }
        if components.modules.is_some() {
            required.insert("component-modules".into());
        }
        required.insert(
            match task.target.kind {
                TargetKind::Native => "native-window",
                TargetKind::Web => "web-dom",
            }
            .into(),
        );
        if !definition.component.states.is_empty()
            || components.modules.as_ref().is_some_and(|modules| {
                modules
                    .modules
                    .modules
                    .values()
                    .any(|module| !module.component.states.is_empty())
            })
        {
            required.insert("component-state".into());
        }
        if stylesheet.mode == StylesheetMode::Tailwind {
            required.insert("tailwind-css".into());
        }
        let bundle = BuildBundle {
            manifest: BuildManifest {
                schema_version: 1,
                session: task.ticket.session.clone(),
                generation: task.ticket.generation,
                graph_hash: task.ticket.graph_hash.clone(),
                graph: task.graph.clone(),
                target: task.target.clone(),
                toolchain: task.toolchain.clone(),
                outputs,
                source_maps,
                available_capabilities: task.available_capabilities.clone(),
                required_capabilities: required,
                diagnostics: if task.target.kind == TargetKind::Native
                    && !compiled.native_full_acceptance
                {
                    vec![BuildDiagnostic {
                        severity: BuildSeverity::Warning,
                        message: "Native CSS full acceptance remains unverified; see build/stylesheet-receipt.json".into(),
                        source: Some(config.stylesheet.entry.clone()),
                        span: None,
                        code: Some("native-css-partial".into()),
                    }]
                } else {
                    Vec::new()
                },
            },
            files,
        };
        bundle.verified_assets()?;
        Ok(bundle)
    }
}
