use super::*;

#[test]
fn target_includes_each_safe_area_edge_once() {
    let placement = RendererClientPlacement {
        viewport: (320, 180),
        insets: [7, 11, 13, 17],
        target: (340, 208),
    };
    assert!(placement.validate().is_ok());
    for target in [(320, 180), (347, 219), (340, 207), (0, 0)] {
        assert_eq!(
            RendererClientPlacement {
                target,
                ..placement
            }
            .validate(),
            Err(SurfaceError::InvalidGeometry)
        );
    }
}

#[test]
fn empty_or_overflowing_placement_is_rejected() {
    for placement in [
        RendererClientPlacement {
            viewport: (0, 180),
            insets: [0; 4],
            target: (0, 180),
        },
        RendererClientPlacement {
            viewport: (320, 0),
            insets: [0; 4],
            target: (320, 0),
        },
        RendererClientPlacement {
            viewport: (u32::MAX, 1),
            insets: [1, 0, 0, 0],
            target: (0, 1),
        },
        RendererClientPlacement {
            viewport: (1, u32::MAX),
            insets: [0, 0, 0, 1],
            target: (1, 0),
        },
    ] {
        assert!(placement.validate().is_err());
    }
}
