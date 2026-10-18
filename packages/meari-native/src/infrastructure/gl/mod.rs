//! Opt-in GL vector paint/readback host. Native handles and GPU objects never enter source IR.
mod platform;
use crate::{
    application::{Renderer, icons},
    domain::{
        Viewport,
        graphics::{Driver, FrameReceipt, FrameTicket, IconCacheKey},
    },
    infrastructure::engine::NativeRenderer,
};
use anyrender_skia::{SkiaSceneCache, SkiaScenePainter};
use glow::HasContext;
use sha2::{Digest, Sha256};
use skia_safe::{
    AlphaType, ColorType, ImageInfo, Surface,
    gpu::{self, DirectContext},
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
static HOST_SEQUENCE: std::sync::Mutex<u64> = std::sync::Mutex::new(1);
pub fn owned_window_count() -> usize {
    platform::owned_window_count()
}

struct CachedIcon {
    image: skia_safe::Image,
    id: u64,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct IconReceipt {
    pub resource_id: u64,
    pub cache_hit: bool,
    pub gpu_backed: bool,
    pub key: IconCacheKey,
}

pub struct GlHost {
    active: Option<Surface>,
    icons: BTreeMap<String, CachedIcon>,
    scene_cache: SkiaSceneCache,
    gpu: Option<DirectContext>,
    platform: platform::Platform,
    generation: u64,
    identity: String,
    scene: u64,
    resource: u64,
    driver: Driver,
    last: Option<(Vec<u8>, FrameReceipt)>,
}

impl GlHost {
    pub fn new(generation: u64) -> Result<Self, String> {
        if generation == 0 {
            return Err("context generation must be nonzero".into());
        }
        let platform = platform::Platform::new()?;
        let gl = unsafe { glow::Context::from_loader_function(|name| platform.address(name)) };
        let driver = unsafe {
            Driver {
                vendor: gl.get_parameter_string(glow::VENDOR),
                renderer: gl.get_parameter_string(glow::RENDERER),
                version: gl.get_parameter_string(glow::VERSION),
            }
        };
        let interface = gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                std::ptr::null()
            } else {
                platform.address(name)
            }
        })
        .ok_or("Skia GL interface unavailable")?;
        let gpu = gpu::direct_contexts::make_gl(interface, None)
            .ok_or("Skia GL GPU context unavailable")?;
        Ok(Self {
            identity: format!(
                "{}-{}-{}",
                std::process::id(),
                {
                    let mut sequence = HOST_SEQUENCE
                        .lock()
                        .map_err(|_| "host identity lock poisoned")?;
                    let value = *sequence;
                    *sequence = sequence.checked_add(1).ok_or("host identity exhausted")?;
                    value
                },
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            ),
            active: None,
            icons: BTreeMap::new(),
            scene_cache: SkiaSceneCache::new(),
            gpu: Some(gpu),
            platform,
            generation,
            scene: 0,
            resource: 0,
            driver,
            last: None,
        })
    }
    pub fn next_ticket(&self) -> Result<FrameTicket, String> {
        Ok(FrameTicket {
            context_id: self.identity.clone(),
            context_generation: self.generation,
            scene_generation: self
                .scene
                .checked_add(1)
                .ok_or("scene generation exhausted")?,
        })
    }
    pub fn last_receipt(&self) -> Option<&FrameReceipt> {
        self.last.as_ref().map(|(_, r)| r)
    }
    pub fn last_pixels(&self) -> Option<&[u8]> {
        self.last.as_ref().map(|(p, _)| p.as_slice())
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    fn surface(&mut self, viewport: &Viewport) -> Result<Surface, String> {
        viewport.validate()?;
        self.platform.make_current()?;
        let info = ImageInfo::new(
            (viewport.width as i32, viewport.height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        gpu::surfaces::render_target(
            self.gpu.as_mut().ok_or("GL context unavailable")?,
            gpu::Budgeted::Yes,
            &info,
            0,
            gpu::SurfaceOrigin::TopLeft,
            None,
            false,
            None,
        )
        .ok_or("GPU surface allocation failed".into())
    }
    fn paint(
        &mut self,
        document: &mut blitz_html::HtmlDocument,
        viewport: &Viewport,
    ) -> Result<Surface, String> {
        if document.get_viewport().window_size != (viewport.width, viewport.height) {
            return Err("GL scene viewport does not match laid-out document".into());
        }
        let mut surface = self.surface(viewport)?;
        surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut painter = SkiaScenePainter::new(surface.canvas(), &mut self.scene_cache);
            blitz_paint::paint_scene(
                &mut painter,
                document,
                1.0,
                viewport.width,
                viewport.height,
                0,
                0,
            );
        }));
        if result.is_err() {
            self.scene_cache = SkiaSceneCache::new();
            return Err("GL scene paint panicked; previous scene retained".into());
        }
        let gpu = self.gpu.as_mut().ok_or("GL context unavailable")?;
        gpu.flush_and_submit();
        if !gpu.submit(gpu::SyncCpu::Yes) {
            return Err("GPU completion submission failed".into());
        }
        self.scene_cache.next_gen();
        Ok(surface)
    }
    pub fn render(
        &mut self,
        document: &mut blitz_html::HtmlDocument,
        viewport: &Viewport,
        ticket: FrameTicket,
    ) -> Result<(Vec<u8>, FrameReceipt), String> {
        if ticket.context_id != self.identity
            || ticket.context_generation != self.generation
            || ticket.scene_generation <= self.scene
        {
            return Err("stale context or scene generation".into());
        }
        let mut surface = self.paint(document, viewport)?;
        let info = ImageInfo::new(
            (viewport.width as i32, viewport.height as i32),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        let mut bytes = vec![0; info.compute_min_byte_size()];
        if !surface.read_pixels(&info, &mut bytes, info.min_row_bytes(), (0, 0)) {
            return Err("GL readback failed; previous scene retained".into());
        }
        let gpu_backed = surface.image_snapshot().is_texture_backed();
        if !gpu_backed {
            return Err("requested GL surface is not GPU-backed".into());
        }
        let receipt = FrameReceipt {
            context_id: self.identity.clone(),
            backend: "skia-opengl-vector".into(),
            context_generation: self.generation,
            scene_generation: ticket.scene_generation,
            width: viewport.width,
            height: viewport.height,
            rgba_sha256: hash(&bytes),
            gpu_backed,
            driver: self.driver.clone(),
        };
        self.scene = ticket.scene_generation;
        self.active = Some(surface);
        self.last = Some((bytes.clone(), receipt.clone()));
        Ok((bytes, receipt))
    }
    pub fn icon_key(
        &self,
        spec: meari::domain::design::icons::IconSpec,
        css: &str,
        scale_milli: u32,
    ) -> IconCacheKey {
        IconCacheKey {
            context_id: self.identity.clone(),
            context_generation: self.generation,
            asset_revision: icons::asset_revision(spec.token),
            style_revision: hash(css.as_bytes()),
            spec,
            scale_milli,
        }
    }
    pub fn cache_icon(&mut self, key: IconCacheKey, css: &str) -> Result<IconReceipt, String> {
        if key.context_id != self.identity
            || key.context_generation != self.generation
            || key.asset_revision != icons::asset_revision(key.spec.token)
            || key.style_revision != hash(css.as_bytes())
            || !(1..=8000).contains(&key.scale_milli)
        {
            return Err("stale or invalid Icon cache identity".into());
        }
        let identity = serde_json::to_string(&key).map_err(|e| e.to_string())?;
        if let Some(entry) = self.icons.get(&identity) {
            return Ok(IconReceipt {
                resource_id: entry.id,
                cache_hit: true,
                gpu_backed: entry.image.is_texture_backed(),
                key,
            });
        }
        let dimension = (u32::from(key.spec.size.pixels()) * key.scale_milli / 1000).max(1);
        let mut icon = NativeRenderer::from_html(&format!(
            "<html><body style=\"margin:0\">{}</body></html>",
            icons::render(key.spec)?
        ));
        // Raw CSS insertion avoids treating source CSS strings as HTML.
        let style = icon.document.mutate().create_element(
            blitz_dom::QualName::new(None, blitz_dom::ns!(html), blitz_dom::local_name!("style")),
            vec![],
        );
        let root = icon.document.root_node().id;
        {
            let mut mutation = icon.document.mutate();
            let text = mutation.create_text_node(css);
            mutation.append_children(style, &[text]);
            mutation.append_children(root, &[style]);
        }
        icon.document.upsert_stylesheet_for_node(style);
        let viewport = Viewport {
            width: dimension,
            height: dimension,
            dark: false,
        };
        icon.layout(&viewport, 0.0)?;
        let mut surface = self.paint(&mut icon.document, &viewport)?;
        let image = surface.image_snapshot();
        if !image.is_texture_backed() {
            return Err("Icon resource is not GPU-backed".into());
        }
        if self.icons.len() >= 128
            && let Some(oldest) = self
                .icons
                .iter()
                .min_by_key(|(_, v)| v.id)
                .map(|(k, _)| k.clone())
        {
            self.icons.remove(&oldest);
        }
        self.resource += 1;
        self.icons.insert(
            identity,
            CachedIcon {
                image,
                id: self.resource,
            },
        );
        Ok(IconReceipt {
            resource_id: self.resource,
            cache_hit: false,
            gpu_backed: true,
            key,
        })
    }
    pub fn recreate(&mut self) -> Result<(), String> {
        let mut next = Self::new(
            self.generation
                .checked_add(1)
                .ok_or("context generation exhausted")?,
        )?;
        next.last = self.last.clone();
        next.identity = self.identity.clone();
        std::mem::swap(self, &mut next);
        drop(next);
        self.platform.make_current()?;
        Ok(())
    }

    pub fn icon_resource_count(&self) -> usize {
        self.icons.len()
    }

    pub fn cached_icon_pixels(&mut self, key: &IconCacheKey) -> Result<Vec<u8>, String> {
        if key.context_id != self.identity || key.context_generation != self.generation {
            return Err("stale Icon context".into());
        }
        let identity = serde_json::to_string(key).map_err(|e| e.to_string())?;
        let image = self
            .icons
            .get(&identity)
            .ok_or("Icon cache entry not found")?
            .image
            .clone();
        let viewport = Viewport {
            width: image.width() as u32,
            height: image.height() as u32,
            dark: false,
        };
        let mut surface = self.surface(&viewport)?;
        surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        surface.canvas().draw_image(&image, (0.0, 0.0), None);
        let gpu = self.gpu.as_mut().ok_or("GL context unavailable")?;
        gpu.flush_and_submit();
        if !gpu.submit(gpu::SyncCpu::Yes) {
            return Err("Icon GPU completion failed".into());
        }
        let info = ImageInfo::new(
            (image.width(), image.height()),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        let mut bytes = vec![0; info.compute_min_byte_size()];
        if !surface.read_pixels(&info, &mut bytes, info.min_row_bytes(), (0, 0)) {
            return Err("cached Icon GPU readback failed".into());
        }
        Ok(bytes)
    }
}
impl Drop for GlHost {
    fn drop(&mut self) {
        if self.platform.make_current().is_ok() {
            if let Some(gpu) = &mut self.gpu {
                gpu.flush_and_submit();
                let _ = gpu.submit(gpu::SyncCpu::Yes);
            }
        } else if let Some(gpu) = &mut self.gpu {
            gpu.abandon();
        }
        self.active = None;
        self.icons.clear();
        self.scene_cache = SkiaSceneCache::new();
        self.gpu = None;
    }
}

thread_local! {static HOST: RefCell<Option<Rc<RefCell<GlHost>>>>=const {RefCell::new(None)};}
pub struct GlScope {
    previous: Option<Rc<RefCell<GlHost>>>,
}
impl GlScope {
    pub fn enter() -> Result<Self, String> {
        let host = Rc::new(RefCell::new(GlHost::new(1)?));
        Ok(Self {
            previous: HOST.with(|current| current.replace(Some(host))),
        })
    }
}
impl Drop for GlScope {
    fn drop(&mut self) {
        HOST.with(|host| {
            host.replace(self.previous.take());
        });
    }
}
pub(crate) fn current() -> Option<Rc<RefCell<GlHost>>> {
    HOST.with(|host| host.borrow().clone())
}
