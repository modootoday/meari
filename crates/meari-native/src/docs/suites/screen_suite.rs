use crate::{
    application::{Renderer, media::MediaSubscriptions, screen::layout_screen},
    domain::Viewport,
    infrastructure::engine::NativeRenderer,
};
use std::path::Path;

pub fn verify(output: &Path) -> Result<(), String> {
    let mut renderer = NativeRenderer::from_html(
        r#"<style>
html,body { margin:0 }
#screen { width:clamp(200px, 50vw, 600px); height:50vh }
#probe { width:17px; height:19px; padding:0; border:0; box-sizing:border-box; background:rgb(255,0,0) }
@media (min-width:400px) { #probe { width:31px } }
@media (orientation:portrait) { #probe { height:37px } }
@media (prefers-color-scheme:dark) { #probe { height:43px } }
</style><div id="screen"></div><button id="probe">Resize</button>"#,
    );
    let mut receipts = Vec::new();
    let mut subscriptions = MediaSubscriptions::default();
    renderer.layout(
        &Viewport {
            width: 1000,
            height: 600,
            dark: false,
        },
        0.0,
    )?;
    let subscription = subscriptions.subscribe(&mut renderer, "(min-width:400px)")?;
    let mut notification_count = 0;
    for (width, height, dark, expected_width, expected_height, probe_width, probe_height) in [
        (1000, 600, false, 500, 300, 31., 19.),
        (600, 1000, false, 300, 500, 17., 37.),
        (1000, 600, true, 500, 300, 31., 43.),
        (1000, 600, false, 500, 300, 31., 19.),
    ] {
        let viewport = layout_screen(
            &mut renderer,
            &Viewport {
                width,
                height,
                dark,
            },
            "screen",
            0.0,
        )?;
        let probe = renderer.bounds("probe")?;
        let changes = subscriptions.refresh(&mut renderer)?;
        notification_count += changes.len();
        if changes
            .iter()
            .any(|change| change.matches != (viewport.width >= 400))
        {
            return Err("media subscription disagrees with CSS viewport".into());
        }
        if !subscriptions.refresh(&mut renderer)?.is_empty() {
            return Err("unchanged media subscription notified twice".into());
        }
        if viewport.width != expected_width
            || viewport.height != expected_height
            || (probe.width - probe_width).abs() > 0.1
            || (probe.height - probe_height).abs() > 0.1
        {
            return Err(format!(
                "screen/media mismatch: {}x{}, probe {}x{}",
                viewport.width, viewport.height, probe.width, probe.height
            ));
        }
        let rgba = renderer.rasterize(&viewport)?;
        if rgba.len() != viewport.width as usize * viewport.height as usize * 4 {
            return Err("screen canvas buffer mismatch".into());
        }
        let offset = ((probe.y as usize + 1) * viewport.width as usize + probe.x as usize + 1) * 4;
        if rgba[offset] < 240 || rgba[offset + 1] > 20 || rgba[offset + 2] > 20 {
            return Err("resized canvas probe pixel mismatch".into());
        }
        receipts.push(serde_json::json!({"viewport":viewport,"probe":probe,"media_changes":changes,"rgba_bytes":rgba.len(),"paint":renderer.last_paint}));
    }
    if notification_count != 2
        || !subscriptions.unsubscribe(subscription.subscription)
        || !subscriptions.refresh(&mut renderer)?.is_empty()
    {
        return Err("media subscription cleanup/count failed".into());
    }
    super::super::write_json(
        output,
        &serde_json::json!({"passed":true,"scenarios":receipts,"os_window_resize_tested":false}),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn screen_and_media_resize_both_directions() {
        let path = std::env::temp_dir().join(format!("bridge-screen-{}.json", std::process::id()));
        super::verify(&path).unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
