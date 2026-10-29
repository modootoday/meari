//! Fixed profile CSS derives from the same Rust geometry consumed by Native input.
use crate::domain::range_parts::RangeParts;
use std::fmt::Write;
pub fn native_stylesheet() -> String {
    stylesheet("")
}
pub fn web_stylesheet(scope: &str) -> Result<String, String> {
    if scope.is_empty() || scope.len() > 4096 || !scope.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("range style requires original hexadecimal document scope".into());
    }
    let prefix = format!("[data-meari-scope='{scope}'] ");
    let mut css = stylesheet(&prefix);
    for profile in [RangeParts::Classic, RangeParts::Compact] {
        let g = profile.geometry();
        let selector = format!(
            "{prefix}input[type=range][data-meari-range-parts='{}']",
            profile.name()
        );
        writeln!(css,"{selector}{{appearance:none;-webkit-appearance:none}}\n{selector}::-webkit-slider-runnable-track{{height:{}px;background:transparent;border:0}}\n{selector}::-webkit-slider-thumb{{-webkit-appearance:none;width:{}px;height:{}px;margin:{}px 0 0;border:0;background:transparent;box-shadow:none}}\n{selector}::-moz-range-track{{height:{}px;background:transparent;border:0}}\n{selector}::-moz-range-thumb{{width:{}px;height:{}px;border:0;background:transparent;box-shadow:none}}\n{selector}::-moz-range-progress{{background:transparent}}",g.hit_px,g.thumb_px,g.thumb_px,(g.hit_px-g.thumb_px)/2,g.hit_px,g.thumb_px,g.thumb_px).expect("String writing");
    }
    Ok(css)
}
fn stylesheet(prefix: &str) -> String {
    let mut css = String::new();
    for profile in [RangeParts::Classic, RangeParts::Compact] {
        let g = profile.geometry();
        let selector = format!(
            "{prefix}input[type=range][data-meari-range-parts='{}']",
            profile.name()
        );
        writeln!(css,"{selector}{{display:inline-block;box-sizing:border-box;height:{}px;min-height:{}px;padding:0;border:0;margin:0;background-color:transparent;\nbackground-image:radial-gradient(circle closest-side,var(--meari-range-thumb-color,currentColor) 0 98%,transparent 100%),linear-gradient(to right,var(--meari-range-fill-color,currentColor) 0%,var(--meari-range-fill-color,currentColor) var(--meari-range-position,0%),var(--meari-range-track-color,#64748b) var(--meari-range-position,0%),var(--meari-range-track-color,#64748b) 100%);\nbackground-size:{}px {}px,calc(100% - {}px) {}px;background-position:var(--meari-range-position,0%) center,center;background-repeat:no-repeat;cursor:pointer}}\n{selector}:disabled{{opacity:.5;cursor:default}}\n{selector}:focus-visible{{outline:2px solid var(--meari-range-focus-color,currentColor);outline-offset:2px}}",g.hit_px,g.hit_px,g.thumb_px,g.thumb_px,g.thumb_px,g.track_px).expect("String writing");
    }
    css
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_css_uses_geometry_and_scopes_without_accepting_raw_css() {
        let native = native_stylesheet();
        assert!(native.contains("height:32px"));
        assert!(native.contains("10px 10px,calc(100% - 10px) 3px"));
        assert!(!native.contains("webkit-slider-thumb"));
        let web = web_stylesheet("0123abcd").unwrap();
        assert!(web.contains("[data-meari-scope='0123abcd'] input"));
        assert!(web.contains("webkit-slider-thumb"));
        assert!(web_stylesheet("x']{}").is_err());
    }
}
