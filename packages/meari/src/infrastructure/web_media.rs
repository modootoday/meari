use crate::application::media::MediaQueryEvaluator;

/// Browser adapter over the same application port as the native CSS engine.
pub struct BrowserMedia;

impl MediaQueryEvaluator for BrowserMedia {
    fn matches_media(&mut self, query: &str) -> Result<bool, String> {
        web_sys::window()
            .ok_or("browser Window is unavailable")?
            .match_media(query)
            .map_err(|error| format!("browser matchMedia failed: {error:?}"))?
            .map(|list| list.matches())
            .ok_or_else(|| "browser did not return a media query list".into())
    }
}
