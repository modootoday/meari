//! Published Vello painter silently omits some operations. Reject them explicitly.
use anyrender::{Paint, Scene, recording::RenderCommand};
pub(super) fn validate(scene: &Scene) -> Result<(), String> {
    for command in &scene.commands {
        let brush = match command {
            RenderCommand::PushLayer(layer) => {
                if layer.filter.is_some() || layer.backdrop_filter.is_some() {
                    return Err(
                        "GPU window backend does not support filter/backdrop-filter; select CPU"
                            .into(),
                    );
                }
                None
            }
            RenderCommand::Fill(command) => Some(&command.brush),
            RenderCommand::Stroke(command) => Some(&command.brush),
            RenderCommand::GlyphRun(command) => Some(&command.brush),
            _ => None,
        };
        if matches!(brush, Some(Paint::Custom(_) | Paint::Resource(_))) {
            return Err(
                "GPU window backend does not support custom/resource brushes; select CPU".into(),
            );
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use anyrender::PaintScene;
    #[test]
    fn unsupported_filters_fail_before_any_gpu_submission_instead_of_disappearing() {
        let mut scene = Scene::new();
        assert!(validate(&scene).is_ok());
        scene.push_layer(
            peniko::Fill::NonZero,
            peniko::BlendMode::default(),
            1.,
            Default::default(),
            &kurbo::Rect::new(0., 0., 50., 50.),
            Some(std::sync::Arc::new(anyrender::Filter::empty())),
            None,
        );
        assert!(
            validate(&scene)
                .unwrap_err()
                .contains("filter/backdrop-filter")
        );
    }
}
