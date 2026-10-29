//! Checked vector scene stage for the native window adapter.
//! This does not certify an OS-presented frame or replace the resource adapter.
use crate::domain::graphics::FrameTicket;
use anyrender::{PaintScene, Scene, WindowRenderer};
use blitz_dom::BaseDocument;

pub(in super::super) struct PreparedScene {
    ticket: FrameTicket,
    width: u32,
    height: u32,
    scene: Scene,
    surface_trace: Option<blitz_paint::SurfacePaintTrace>,
}

/// A successful renderer submission is distinct from OS presentation/readback.
pub(in super::super) struct SceneSubmission {
    pub surface_trace: Option<blitz_paint::SurfacePaintTrace>,
    pub ticket: FrameTicket,
    pub width: u32,
    pub height: u32,
    pub commands: usize,
}

impl PreparedScene {
    pub fn prepare(
        document: &mut BaseDocument,
        ticket: FrameTicket,
        insets: (u32, u32),
    ) -> Result<Self, String> {
        Self::prepare_inner(document, ticket, insets, None)
    }
    pub fn prepare_observed(
        document: &mut BaseDocument,
        ticket: FrameTicket,
        insets: (u32, u32),
        targets: &[(u64, u64)],
    ) -> Result<Self, String> {
        Self::prepare_inner(document, ticket, insets, Some(targets))
    }
    fn prepare_inner(
        document: &mut BaseDocument,
        ticket: FrameTicket,
        insets: (u32, u32),
        targets: Option<&[(u64, u64)]>,
    ) -> Result<Self, String> {
        if ticket.context_id.is_empty()
            || ticket.context_generation == 0
            || ticket.scene_generation == 0
        {
            return Err("checked scene requires a live context and source generation".into());
        }
        let viewport = document.viewport();
        let (width, height) = viewport.window_size;
        let scale = viewport.scale_f64();
        if width == 0 || height == 0 || !scale.is_finite() || scale <= 0.0 {
            return Err("checked scene requires nonzero bounds and a finite positive scale".into());
        }
        if document.has_pending_critical_resources() {
            return Err("checked scene is waiting for critical resources".into());
        }
        let (scene, surface_trace) = if let Some(targets) = targets {
            blitz_paint::prepare_scene_observed(document, scale, (width, height), insets, targets)
                .map_err(str::to_owned)?
        } else {
            (
                blitz_paint::prepare_scene_checked(
                    document, scale, width, height, insets.0, insets.1,
                )
                .map_err(str::to_owned)?,
                None,
            )
        };
        Ok(Self {
            ticket,
            width,
            height,
            scene,
            surface_trace,
        })
    }

    /// The owning event loop must hold exclusive source/context ownership across this call.
    /// FrameTicket is a DTO, not an ownership lease; an asynchronous adapter must
    /// query its live owner rather than reusing the prepared ticket as `current`.
    /// Failure returns before opening the target render/present transaction.
    pub fn submit(
        self,
        current: &FrameTicket,
        bounds: (u32, u32),
        renderer: &mut impl WindowRenderer,
    ) -> Result<SceneSubmission, String> {
        if &self.ticket != current || bounds != (self.width, self.height) {
            return Err(
                "prepared vector scene is stale after source, context or viewport change".into(),
            );
        }
        if !renderer.is_active() || renderer.is_pending() {
            return Err("checked scene renderer is not active".into());
        }
        let commands = self.scene.commands.len();
        renderer.render(|target| target.append_scene(self.scene, Default::default()));
        Ok(SceneSubmission {
            surface_trace: self.surface_trace,
            ticket: self.ticket,
            width: self.width,
            height: self.height,
            commands,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::Renderer, domain::Viewport, infrastructure::engine::NativeRenderer};
    use anyrender::{ImageRenderer, RenderContext};
    use std::sync::Arc;

    struct Target {
        active: bool,
        submissions: usize,
        scene: Scene,
    }
    impl RenderContext for Target {}
    impl WindowRenderer for Target {
        type ScenePainter<'a> = Scene;
        fn resume<F: FnOnce() + 'static>(
            &mut self,
            _: Arc<dyn anyrender::WindowHandle>,
            _: u32,
            _: u32,
            ready: F,
        ) {
            self.active = true;
            ready();
        }
        fn complete_resume(&mut self) -> bool {
            self.active
        }
        fn suspend(&mut self) {
            self.active = false;
        }
        fn is_active(&self) -> bool {
            self.active
        }
        fn set_size(&mut self, _: u32, _: u32) {}
        fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw: F) {
            let mut next = Scene::new();
            draw(&mut next);
            self.scene = next;
            self.submissions += 1;
        }
    }
    fn ticket() -> FrameTicket {
        FrameTicket {
            context_id: "window-test".into(),
            context_generation: 1,
            scene_generation: 1,
        }
    }
    fn document() -> NativeRenderer {
        let mut renderer = NativeRenderer::from_html(
            "<!doctype html><style>body{margin:0}div{width:100px;height:50px;background:red;color:white}</style><div>checked text</div>",
        );
        renderer
            .layout(
                &Viewport {
                    width: 120,
                    height: 80,
                    dark: false,
                },
                0.0,
            )
            .unwrap();
        renderer
    }
    #[test]
    fn vector_prepare_replay_matches_direct_cpu_pixels() {
        let mut renderer = document();
        let prepared = PreparedScene::prepare(&mut renderer.document, ticket(), (0, 0)).unwrap();
        let mut recorded_pixels = Vec::new();
        let mut recorded = anyrender_vello_cpu::VelloCpuImageRenderer::new(120, 80);
        recorded.render_to_vec(
            |target| target.append_scene(prepared.scene, Default::default()),
            &mut recorded_pixels,
        );
        let mut direct_pixels = Vec::new();
        let mut direct = anyrender_vello_cpu::VelloCpuImageRenderer::new(120, 80);
        direct.render_to_vec(
            |target| {
                blitz_paint::paint_scene_checked(target, &mut renderer.document, 1.0, 120, 80, 0, 0)
                    .unwrap()
            },
            &mut direct_pixels,
        );
        assert_eq!(recorded_pixels, direct_pixels);
        assert!(
            recorded_pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] > 200 && pixel[1] < 20)
        );
    }
    #[test]
    fn stale_or_inactive_submission_preserves_previous_scene_without_opening_renderer() {
        let mut renderer = document();
        let mut target = Target {
            active: true,
            submissions: 0,
            scene: Scene::new(),
        };
        let prepared = PreparedScene::prepare(&mut renderer.document, ticket(), (0, 0)).unwrap();
        let submitted = prepared.submit(&ticket(), (120, 80), &mut target).unwrap();
        assert_eq!(submitted.ticket, ticket());
        assert_eq!((submitted.width, submitted.height), (120, 80));
        assert!(submitted.commands > 0);
        let previous = target.scene.clone();
        for change in 0..3 {
            let prepared =
                PreparedScene::prepare(&mut renderer.document, ticket(), (0, 0)).unwrap();
            let mut current = ticket();
            let mut bounds = (120, 80);
            match change {
                0 => current.context_generation += 1,
                1 => current.scene_generation += 1,
                _ => bounds.0 += 1,
            }
            assert!(prepared.submit(&current, bounds, &mut target).is_err());
            assert_eq!(target.submissions, 1);
            assert_eq!(target.scene, previous);
        }
        target.active = false;
        let prepared = PreparedScene::prepare(&mut renderer.document, ticket(), (0, 0)).unwrap();
        assert!(prepared.submit(&ticket(), (120, 80), &mut target).is_err());
        assert_eq!(target.submissions, 1);
        assert_eq!(target.scene, previous);
    }

    #[test]
    fn actual_svg_tile_failure_leaves_the_previously_submitted_scene_intact() {
        use crate::infrastructure::document::InteractiveDocument;
        use meari::{
            application::{Language, assets::VerifiedAssets},
            domain::{
                assets::{AssetDescriptor, AssetManifest},
                build::{BuildPath, ContentHash, InputRole},
            },
        };
        use std::collections::BTreeMap;
        let bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\"><rect width=\"24\" height=\"24\" fill=\"red\"/></svg>";
        let hash = ContentHash::of(bytes);
        let descriptor = AssetDescriptor {
            role: InputRole::Icon,
            content: hash.clone(),
            length: bytes.len() as u64,
            media_type: "image/svg+xml".into(),
            license: None,
        };
        let url = format!("/{}", descriptor.output_path().unwrap().as_str());
        let path: BuildPath = "pattern.svg".to_owned().try_into().unwrap();
        let assets = VerifiedAssets::new(
            AssetManifest {
                schema_version: 1,
                files: BTreeMap::from([(path, descriptor)]),
            },
            BTreeMap::from([(hash, bytes.to_vec())]),
        )
        .unwrap();
        let component = meari::parse(
            "type Props={};export function Pattern(props:Props){return <main/>;}",
            Language::Tsx,
        )
        .unwrap();
        let css = format!(
            "main{{width:60px;height:40px;background-image:url('{url}');background-size:0.01px 0.01px}}"
        );
        let mut failing = InteractiveDocument::new_with_assets(
            component,
            &css,
            Viewport {
                width: 160,
                height: 120,
                dark: false,
            },
            assets,
        )
        .unwrap();
        let mut good = document();
        let mut target = Target {
            active: true,
            submissions: 0,
            scene: Scene::new(),
        };
        PreparedScene::prepare(&mut good.document, ticket(), (0, 0))
            .unwrap()
            .submit(&ticket(), (120, 80), &mut target)
            .unwrap();
        let previous = target.scene.clone();
        let error = PreparedScene::prepare(&mut failing.renderer.document, ticket(), (0, 0))
            .err()
            .unwrap();
        assert!(error.contains("vector tile budget"));
        assert_eq!(target.submissions, 1);
        assert_eq!(target.scene, previous);
    }
    #[test]
    fn observed_prepare_keeps_same_scene_and_stale_submit_returns_no_observation() {
        let mut document = document();
        let node = document.document.root_element().id.as_u64();
        let prepared =
            PreparedScene::prepare_observed(&mut document.document, ticket(), (0, 0), &[(node, 1)])
                .unwrap();
        let mut current = ticket();
        current.scene_generation += 1;
        let mut target = Target {
            active: true,
            submissions: 0,
            scene: Scene::new(),
        };
        assert!(prepared.submit(&current, (120, 80), &mut target).is_err());
        assert_eq!(target.submissions, 0);
        let submitted =
            PreparedScene::prepare_observed(&mut document.document, ticket(), (0, 0), &[(node, 1)])
                .unwrap()
                .submit(&ticket(), (120, 80), &mut target)
                .unwrap();
        assert!(submitted.surface_trace.is_some());
        assert_eq!(target.submissions, 1);
    }
}
