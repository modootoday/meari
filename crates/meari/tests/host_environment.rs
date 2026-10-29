use meari::{
    application::screen::{ResizeSurface, ScreenRequest, measured_viewport},
    domain::{
        host::{HostCapabilities, HostEnvironment, HostTarget, Insets, Support, VisualViewport},
        viewport::Viewport,
    },
};

fn host(target: HostTarget) -> HostEnvironment {
    HostEnvironment {
        target,
        layout: Viewport {
            width: 800,
            height: 600,
            dark: false,
        },
        visual: VisualViewport {
            width: 400.0,
            height: 300.0,
            offset_x: 20.0,
            offset_y: 30.0,
            scale: 2.0,
        },
        device_scale: 1.5,
        layout_zoom: Some(1.0),
        safe_area: Some(Insets {
            top: 0.0,
            right: 0.0,
            bottom: 20.0,
            left: 0.0,
        }),
        capabilities: HostCapabilities {
            client_resize: Support::Supported,
            preview_resize: Support::Unknown,
            reduced_motion: Support::Unknown,
            contrast: Support::Unknown,
            pointer_hover: Support::Unknown,
            accessibility: Support::Unknown,
        },
    }
}

#[test]
fn snapshots_preserve_distinct_layout_visual_and_device_scales() {
    let original = host(HostTarget::Web);
    let decoded: HostEnvironment =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded.layout.width, 800);
    assert_eq!(decoded.visual.width, 400.0);
    assert_eq!(decoded.visual.scale, 2.0);
    assert_eq!(decoded.device_scale, 1.5);
    assert_eq!(decoded.capabilities.reduced_motion, Support::Unknown);
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut value = original.clone();
        value.device_scale = invalid;
        assert!(value.validate().is_err());
        value = original.clone();
        value.visual.scale = invalid;
        assert!(value.validate().is_err());
        value = original.clone();
        value.visual.height = invalid;
        assert!(value.validate().is_err());
        value = original.clone();
        value.layout_zoom = Some(invalid);
        assert!(value.validate().is_err());
    }
    let mut value = original;
    value.safe_area.as_mut().unwrap().bottom = -1.0;
    assert!(value.validate().is_err());
    value.safe_area.as_mut().unwrap().bottom = 0.0;
    value.visual.offset_x = f64::NAN;
    assert!(value.validate().is_err());
}

#[test]
fn resize_requires_matching_target_and_explicit_capability() {
    let mut environment = host(HostTarget::Native);
    let mut request = ScreenRequest {
        surface: ResizeSurface::NativeClient,
        viewport: measured_viewport(500.5, 300.2, true).unwrap(),
    };
    request.validate_for(&environment).unwrap();
    assert_eq!(request.viewport.width, 501);
    assert!(request.viewport.dark);
    for support in [Support::Unknown, Support::Unsupported] {
        environment.capabilities.client_resize = support;
        assert!(request.validate_for(&environment).is_err());
    }
    environment.capabilities.preview_resize = Support::Supported;
    request.surface = ResizeSurface::WebPreview;
    assert!(request.validate_for(&environment).is_err());
    environment.target = HostTarget::Web;
    request.validate_for(&environment).unwrap();
    request.viewport.width = 0;
    assert!(request.validate_for(&environment).is_err());
    for invalid in [0.0, 0.99, 8192.01, f64::NAN, f64::INFINITY] {
        assert!(measured_viewport(invalid, 300.0, false).is_err());
        assert!(measured_viewport(500.0, invalid, false).is_err());
    }
}

#[test]
fn resize_acknowledgements_reject_stale_invalid_and_cross_target_observations() {
    use meari::application::screen::resize::{ResizeStatus, ResizeTransaction};
    let mut observed = host(HostTarget::Native);
    let request = ScreenRequest {
        surface: ResizeSurface::NativeClient,
        viewport: measured_viewport(500.0, 300.0, false).unwrap(),
    };
    let mut transaction = ResizeTransaction::default();
    let first = transaction.begin(request.clone(), &observed).unwrap();
    assert!(transaction.begin(request.clone(), &observed).is_err());
    assert!(
        transaction
            .acknowledge(first + 1, observed.clone())
            .is_err()
    );
    observed.target = HostTarget::Web;
    assert!(transaction.acknowledge(first, observed.clone()).is_err());
    observed.target = HostTarget::Native;
    observed.device_scale = f64::NAN;
    assert!(transaction.acknowledge(first, observed.clone()).is_err());
    observed.device_scale = 1.5;
    assert_eq!(
        transaction
            .acknowledge(first, observed.clone())
            .unwrap()
            .status,
        ResizeStatus::Adjusted
    );
    let second = transaction.begin(request.clone(), &observed).unwrap();
    assert!(transaction.reject(first).is_err());
    transaction.reject(second).unwrap();
    let third = transaction.begin(request, &observed).unwrap();
    assert!(third > second);
    observed.layout.width = 500;
    observed.layout.height = 300;
    observed.capabilities.client_resize = Support::Unsupported;
    assert_eq!(
        transaction
            .acknowledge(third, observed.clone())
            .unwrap()
            .status,
        ResizeStatus::Applied
    );
    assert!(transaction.acknowledge(third, observed).is_err());
}
