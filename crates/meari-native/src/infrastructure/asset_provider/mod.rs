//! Synchronous declared-memory resource delivery; never a filesystem/network fallback.
use blitz_traits::net::{Body, Bytes, Method, NetHandler, NetProvider, Request};
use meari::{application::assets::VerifiedAssets, domain::build::BuildPath};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
mod decode;

pub(super) const BASE_URL: &str = "https://meari.invalid/";

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct AssetLoadReport {
    pub document: Option<usize>,
    pub requests: u64,
    /// Bytes delivered to Blitz's handler, not an assertion of decoded DOM readiness.
    pub delivered: BTreeMap<String, u64>,
    pub rejected: u64,
    pub failures: Vec<AssetRejection>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct AssetRejection {
    pub path: String,
    pub reason: String,
}

pub(super) struct AssetProvider {
    assets: VerifiedAssets,
    report: Mutex<AssetLoadReport>,
}
impl AssetProvider {
    pub fn validate_reference(&self, reference: &str) -> Result<(), String> {
        let url = blitz_traits::net::Url::parse(BASE_URL)
            .and_then(|base| base.join(reference))
            .map_err(|error| error.to_string())?;
        self.lookup(&url).map(|_| ())
    }
    pub fn validate_css(&self, css: &str) -> Result<(), String> {
        use meari::domain::{
            assets::AssetManifest,
            design::stylesheet::{StylesheetMode, StylesheetProject},
        };
        let manifest = AssetManifest {
            schema_version: 1,
            files: self
                .assets
                .manifest()
                .files
                .values()
                .map(|descriptor| Ok((descriptor.output_path()?, descriptor.clone())))
                .collect::<Result<_, String>>()?,
        };
        super::stylesheet_assets::rewrite(
            &StylesheetProject::inline(StylesheetMode::Css, css),
            &manifest,
        )
        .map_err(|error| error.to_string())?;
        if !super::stylesheet::imports(css, StylesheetMode::Css)?.is_empty() {
            return Err("asset stylesheet imports must be resolved by the project compiler".into());
        }
        Ok(())
    }
    pub fn new(assets: VerifiedAssets) -> Result<Arc<Self>, String> {
        let mut seen = BTreeSet::new();
        for descriptor in assets.manifest().files.values() {
            let path = descriptor.output_path()?;
            if seen.insert(path.clone()) {
                let (media_type, bytes) = assets.resource(&path).ok_or("asset resource missing")?;
                decode::validate(media_type, bytes)
                    .map_err(|error| format!("{}: {error}", path.as_str()))?;
            }
        }
        Ok(Arc::new(Self {
            assets,
            report: Mutex::new(AssetLoadReport::default()),
        }))
    }
    pub fn report(&self) -> Result<AssetLoadReport, String> {
        self.report
            .lock()
            .map(|report| report.clone())
            .map_err(|_| "asset report is poisoned".into())
    }
    pub fn ensure_success(&self) -> Result<(), String> {
        let report = self.report.lock().map_err(|_| "asset report is poisoned")?;
        if report.rejected != 0 {
            return Err("declared asset delivery failed; inspect asset report".into());
        }
        Ok(())
    }
    pub fn request_count(&self) -> Result<u64, String> {
        self.report
            .lock()
            .map(|report| report.requests)
            .map_err(|_| "asset report is poisoned".into())
    }
    fn resource(&self, document: usize, request: &Request) -> Result<(&str, &[u8]), String> {
        let mut report = self.report.lock().map_err(|_| "asset report is poisoned")?;
        report.requests = report.requests.saturating_add(1);
        if report.requests > 65536 {
            return Err("asset request budget exceeded".into());
        }
        match report.document {
            Some(current) if current != document => {
                return Err("foreign document asset request".into());
            }
            None => report.document = Some(document),
            _ => {}
        }
        if request
            .signal
            .as_ref()
            .is_some_and(|signal| signal.aborted())
        {
            return Err("asset request was aborted".into());
        }
        if request.method != Method::GET || !matches!(&request.body, Body::Empty) {
            return Err("asset resource requires GET without a body".into());
        }
        let result = self.lookup(&request.url)?;
        *report
            .delivered
            .entry(request.url.path().trim_start_matches('/').to_owned())
            .or_default() += 1;
        Ok(result)
    }
    fn lookup(&self, url: &blitz_traits::net::Url) -> Result<(&str, &[u8]), String> {
        if url.as_str().len() > 8192
            || url.scheme() != "https"
            || url.host_str() != Some("meari.invalid")
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.path().contains('%')
        {
            return Err("asset URL authority or representation is not registered".into());
        }
        let path = BuildPath::try_from(
            url.path()
                .strip_prefix('/')
                .ok_or("asset URL path missing")?
                .to_owned(),
        )?;
        let (media, bytes) = self
            .assets
            .resource(&path)
            .ok_or("asset URL is not declared")?;
        Ok((media, bytes))
    }
}
impl NetProvider for AssetProvider {
    fn fetch(&self, document: usize, request: Request, handler: Box<dyn NetHandler>) {
        let resolved = request.url.as_str().to_owned();
        match self.resource(document, &request) {
            Ok((_, bytes)) => handler.bytes(resolved, Bytes::copy_from_slice(bytes)),
            Err(reason) => {
                if let Ok(mut report) = self.report.lock() {
                    report.rejected = report.rejected.saturating_add(1);
                    if report.failures.len() < 256 {
                        report.failures.push(AssetRejection {
                            path: request.url.path().chars().take(1024).collect(),
                            reason,
                        });
                    }
                }
                // Complete Blitz's pending request with a decode failure instead of
                // leaving a render-blocking request waiting forever. The host checks
                // ensure_success and must not publish that frame as successful.
                handler.bytes(resolved, Bytes::new());
            }
        }
    }
}

#[cfg(test)]
mod tests;
