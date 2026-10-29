use super::*;
#[test]
fn physical_bounds_round_once_without_an_extra_scale() {
    let rect = PhysicalClientRect {
        x: 10.25,
        y: -2.75,
        width: 20.5,
        height: 8.25,
    };
    assert_eq!(rect.enclosing_i32().unwrap(), [10, -3, 21, 9]);
}
#[test]
fn invalid_or_unrepresentable_geometry_is_not_a_surface() {
    for rect in [
        PhysicalClientRect {
            x: f64::NAN,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        PhysicalClientRect {
            x: 0.0,
            y: 0.0,
            width: -1.0,
            height: 1.0,
        },
        PhysicalClientRect {
            x: f64::from(i32::MAX) + 1.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
    ] {
        assert!(rect.enclosing_i32().is_err());
    }
}
#[test]
fn zero_area_stays_zero_and_right_edge_overflow_is_rejected() {
    assert_eq!(
        PhysicalClientRect {
            x: 0.25,
            y: 0.25,
            width: 0.0,
            height: 0.0
        }
        .enclosing_i32()
        .unwrap(),
        [0, 0, 0, 0]
    );
    assert!(
        PhysicalClientRect {
            x: f64::from(i32::MAX) - 1.0,
            y: 0.0,
            width: 3.0,
            height: 1.0
        }
        .enclosing_i32()
        .is_err()
    );
}
