//! Real GL lifecycle/cache oracle, independent of strict upstream compatibility acceptance.
use crate::{
    application::Renderer,
    domain::{Viewport, graphics::FrameTicket},
    infrastructure::{artifacts, engine::NativeRenderer, gl::GlHost},
};
use meari::domain::design::icons::{IconSpec, IconToken};
use std::path::Path;

pub fn verify(output: &Path) -> Result<(), String> {
    let windows_before = crate::infrastructure::gl::owned_window_count();
    let mut host = GlHost::new(1)?;
    let mut source = NativeRenderer::from_html(
        "<html><body style='margin:0;background:white'><div style='width:32px;height:24px;background:red'></div></body></html>",
    );
    let viewport = Viewport {
        width: 64,
        height: 48,
        dark: false,
    };
    source.layout(&viewport, 0.0)?;
    let (pixels, first) = host.render(&mut source.document, &viewport, host.next_ticket()?)?;
    if !first.gpu_backed
        || pixels[0] < 240
        || pixels[1] > 20
        || pixels[(40 * 64 + 40) * 4] < 240
        || pixels[(40 * 64 + 40) * 4 + 1] < 240
    {
        return Err("GL color/orientation readback oracle failed".into());
    }
    let old_pixels = host.last_pixels().unwrap().to_vec();
    if host
        .render(
            &mut source.document,
            &viewport,
            FrameTicket {
                context_id: first.context_id.clone(),
                context_generation: 1,
                scene_generation: 1,
            },
        )
        .is_ok()
    {
        return Err("stale scene accepted".into());
    }
    let invalid = Viewport {
        width: 0,
        height: 48,
        dark: false,
    };
    if host
        .render(&mut source.document, &invalid, host.next_ticket()?)
        .is_ok()
        || host.last_pixels() != Some(old_pixels.as_slice())
    {
        return Err("failed frame replaced last good scene".into());
    }
    let css = "body{margin:0}svg{color:#c00000;width:100%;height:100%}";
    let key = host.icon_key(IconSpec::new(IconToken::SelectionCheck), css, 1000);
    let created = host.cache_icon(key.clone(), css)?;
    let hit = host.cache_icon(key.clone(), css)?;
    if created.cache_hit
        || !hit.cache_hit
        || created.resource_id != hit.resource_id
        || !hit.gpu_backed
    {
        return Err("GL Icon GPU cache reuse failed".into());
    }
    let mut invalid_asset = key.clone();
    invalid_asset.asset_revision = "stale".into();
    if host.cache_icon(invalid_asset, css).is_ok() {
        return Err("stale asset revision accepted".into());
    }
    let cached_pixels = host.cached_icon_pixels(&key)?;
    if !cached_pixels
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p[0] > 100 && p[1] < 20 && p[2] < 20 && p[3] > 100)
    {
        return Err("cached GPU Icon has no expected red geometry".into());
    }
    let mut foreign = key.clone();
    foreign.context_id = "other-host".into();
    if host.cache_icon(foreign, css).is_ok() {
        return Err("foreign host Icon key accepted".into());
    }
    let other = host.cache_icon(
        host.icon_key(key.spec, "svg{color:blue}", 1000),
        "svg{color:blue}",
    )?;
    if other.cache_hit || other.resource_id == created.resource_id {
        return Err("Icon style revision aliased".into());
    }
    let scaled = host.cache_icon(host.icon_key(key.spec, css, 2000), css)?;
    if scaled.cache_hit || scaled.resource_id == created.resource_id {
        return Err("Icon scale aliased".into());
    }
    host.recreate()?;
    if host.generation() != 2
        || host.last_pixels() != Some(old_pixels.as_slice())
        || host.cache_icon(key, css).is_ok()
    {
        return Err("context recovery/stale resource rejection failed".into());
    }
    let rebuilt = host.cache_icon(
        host.icon_key(IconSpec::new(IconToken::SelectionCheck), css, 1000),
        css,
    )?;
    if rebuilt.cache_hit || !rebuilt.gpu_backed {
        return Err("GPU Icon not rebuilt after context recreation".into());
    }
    let (_, second) = host.render(&mut source.document, &viewport, host.next_ticket()?)?;
    if second.context_generation != 2 || second.scene_generation != 1 {
        return Err("new context scene generation did not restart".into());
    }
    let budget_key = host.icon_key(IconSpec::new(IconToken::SelectionCheck), css, 1000);
    for scale in 1001..1130 {
        host.cache_icon(host.icon_key(budget_key.spec, css, scale), css)?;
    }
    if host.icon_resource_count() != 128 || host.cached_icon_pixels(&budget_key).is_ok() {
        return Err("Icon cache budget/eviction failed".into());
    }
    drop(host);
    if crate::infrastructure::gl::owned_window_count() != windows_before {
        return Err("owned GL bootstrap window leaked after teardown".into());
    }
    let checks = [
        "gpu-color-orientation",
        "stale-scene",
        "failed-frame-preserves-last",
        "gpu-icon-hit",
        "asset-revision-rejected",
        "cached-gpu-icon-color",
        "foreign-host-rejected",
        "style-identity",
        "scale-identity",
        "context-recreation-stale-key",
        "icon-reupload",
        "new-context-scene-generation",
        "icon-budget-eviction",
        "host-window-teardown",
    ];
    artifacts::json(
        output,
        &serde_json::json!({"passed":true,"assertions":checks.len(),"checks":checks,"first":first,"recreated":second,"icon_created":created,"icon_hit":hit,"style_changed":other,"scale_changed":scaled,"icon_rebuilt":rebuilt,"owned_windows_before":windows_before,"owned_windows_after":crate::infrastructure::gl::owned_window_count(),"actual_driver_loss_tested":false,"dom_svg_automatic_cache":false}),
    )
}

#[cfg(test)]
mod tests {
    static GL_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    #[test]
    #[ignore = "requires actual Windows GL driver"]
    fn gl_scope_binds_documents_and_restores_cpu_default() {
        let _guard = GL_TEST.lock().expect("GL test isolation lock");
        use crate::application::Renderer;
        let viewport = crate::domain::Viewport {
            width: 32,
            height: 32,
            dark: false,
        };
        let before = crate::infrastructure::gl::owned_window_count();
        {
            let _scope = crate::infrastructure::gl::GlScope::enter().unwrap();
            let mut renderer =
                crate::infrastructure::engine::NativeRenderer::from_html("<div>GL</div>");
            renderer.layout(&viewport, 0.0).unwrap();
            renderer.rasterize(&viewport).unwrap();
            assert!(renderer.last_paint.as_ref().unwrap().gpu_backed);
        }
        assert_eq!(crate::infrastructure::gl::owned_window_count(), before);
        let mut renderer =
            crate::infrastructure::engine::NativeRenderer::from_html("<div>CPU</div>");
        renderer.layout(&viewport, 0.0).unwrap();
        renderer.rasterize(&viewport).unwrap();
        assert!(renderer.last_paint.is_none());
    }
    #[test]
    #[ignore = "requires actual Windows GL driver"]
    fn actual_gl_lifecycle_cache_and_teardown() {
        let _guard = GL_TEST.lock().expect("GL test isolation lock");
        let path = std::env::temp_dir().join(format!(
            "bridge-gl-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        super::verify(&path).unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
