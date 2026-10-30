use super::*;
use blitz_traits::shell::ColorScheme;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct CountingShellProvider {
    redraw_count: AtomicUsize,
}
impl ShellProvider for CountingShellProvider {
    fn request_redraw(&self) {
        self.redraw_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn zoom_requests_redraw() {
    let shell_provider = Arc::new(CountingShellProvider::default());
    let mut doc = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
        shell_provider: Some(shell_provider.clone() as _),
        ..Default::default()
    });
    doc.resolve(0.0);

    let base = shell_provider.redraw_count.load(Ordering::SeqCst);
    doc.zoom_by(0.5);
    assert!(shell_provider.redraw_count.load(Ordering::SeqCst) > base);

    let base = shell_provider.redraw_count.load(Ordering::SeqCst);
    doc.zoom_to(1.0);
    assert!(shell_provider.redraw_count.load(Ordering::SeqCst) > base);
}
