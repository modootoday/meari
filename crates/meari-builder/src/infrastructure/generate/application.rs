//! Generated host factories share the same application or module-application definition.
use meari::domain::build::TargetKind;

pub(super) fn source(target: TargetKind, modules: bool) -> &'static str {
    match (target, modules) {
        (TargetKind::Native, false) => NATIVE,
        (TargetKind::Native, true) => NATIVE_MODULES,
        (TargetKind::Web, false) => WEB,
        (TargetKind::Web, true) => WEB_MODULES,
    }
}

const NATIVE: &str = r#"use meari::application::app_bundle::ApplicationDefinition;
use meari_native::infrastructure::document::InteractiveDocument;
pub const MODULES:bool=false;
pub fn mount()->Result<(ApplicationDefinition,InteractiveDocument),String> {
    let definition:ApplicationDefinition=serde_json::from_str(include_str!("../app.json")).map_err(|error|error.to_string())?;
    definition.validate_target(meari::domain::build::TargetKind::Native)?;
    let document=match super::assets::load()? {
        Some(assets)=>InteractiveDocument::from_application_with_assets(&definition,include_str!("../styles.css"),assets)?,
        None=>InteractiveDocument::from_application(&definition,include_str!("../styles.css"))?,
    };
    Ok((definition,document))
}
"#;

const NATIVE_MODULES: &str = r#"use meari::application::{app_bundle::ApplicationDefinition,module_app_bundle::ModuleApplicationDefinition};
use meari_native::infrastructure::document::InteractiveDocument;
pub const MODULES:bool=true;
pub fn mount()->Result<(ApplicationDefinition,InteractiveDocument),String> {
    let definition:ModuleApplicationDefinition=serde_json::from_str(include_str!("../module-app.json")).map_err(|error|error.to_string())?;
    let document=match super::assets::load()? {
        Some(assets)=>InteractiveDocument::from_module_application_with_assets(&definition,include_str!("../styles.css"),assets)?,
        None=>InteractiveDocument::from_module_application(&definition,include_str!("../styles.css"))?,
    };
    Ok((definition.application,document))
}
"#;

const WEB: &str = r#"use meari::{application::app_bundle::ApplicationDefinition,infrastructure::web_document::WebDocument};
pub fn mount()->Result<WebDocument,String> {
    super::assets::validate()?;
    let definition:ApplicationDefinition=serde_json::from_str(include_str!("../app.json")).map_err(|error|error.to_string())?;
    WebDocument::mount_application(&definition,include_str!("../styles.css"))
}
"#;

const WEB_MODULES: &str = r#"use meari::{application::module_app_bundle::ModuleApplicationDefinition,infrastructure::web_document::WebDocument};
pub fn mount()->Result<WebDocument,String> {
    super::assets::validate()?;
    let definition:ModuleApplicationDefinition=serde_json::from_str(include_str!("../module-app.json")).map_err(|error|error.to_string())?;
    WebDocument::mount_module_application(&definition,include_str!("../styles.css"))
}
"#;
