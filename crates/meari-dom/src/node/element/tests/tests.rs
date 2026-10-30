use super::TextInputData;
use parley::{FontContext, LayoutContext};

/// Build a [`TextInputData`] with the given text laid out at scale 1.0.
fn make_input(is_multiline: bool, text: &str) -> TextInputData {
    let mut font_ctx = FontContext::new();
    let mut layout_ctx = LayoutContext::new();
    let mut data = TextInputData::new(is_multiline);
    data.editor.set_scale(1.0);
    data.editor.set_text(text);
    data.editor
        .driver(&mut font_ctx, &mut layout_ctx)
        .refresh_layout();
    data
}

#[test]
fn short_text_does_not_scroll() {
    let mut data = make_input(false, "hi");
    // A wide content box that comfortably fits the text.
    data.clamp_scroll_offset(1000.0, 100.0);
    assert_eq!(data.scroll_offset, 0.0);
}

#[test]
fn single_line_scrolls_to_follow_caret() {
    let text = "the quick brown fox jumps over the lazy dog repeatedly and at length";
    let mut data = make_input(false, text);
    let content_box_width = 40.0;
    let content_box_height = 20.0;

    // Caret at the end of a string that overflows a narrow input should scroll right.
    data.editor
        .driver(&mut FontContext::new(), &mut LayoutContext::new())
        .move_to_text_end();
    data.clamp_scroll_offset(content_box_width, content_box_height);

    let layout_width = data.editor.try_layout().unwrap().full_width();
    if layout_width > content_box_width {
        assert!(
            data.scroll_offset > 0.0,
            "expected horizontal scroll for overflowing single-line input"
        );
        // The caret must be within the visible region after scrolling.
        let caret = data.editor.cursor_geometry(1.5).unwrap();
        assert!(caret.x1 as f32 <= data.scroll_offset + content_box_width + 0.5);
        assert!(caret.x0 as f32 >= data.scroll_offset - 0.5);
    }

    // Moving the caret back to the start should reset the scroll offset.
    data.editor
        .driver(&mut FontContext::new(), &mut LayoutContext::new())
        .move_to_text_start();
    data.clamp_scroll_offset(content_box_width, content_box_height);
    assert_eq!(data.scroll_offset, 0.0);
}

#[test]
fn multiline_scrolls_vertically_not_horizontally() {
    let text = (0..40)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut data = make_input(true, &text);
    // Constrain the width so wrapping is well-defined.
    data.editor.set_width(Some(200.0));
    data.editor
        .driver(&mut FontContext::new(), &mut LayoutContext::new())
        .refresh_layout();

    let content_box_width = 200.0;
    let content_box_height = 30.0;

    data.editor
        .driver(&mut FontContext::new(), &mut LayoutContext::new())
        .move_to_text_end();
    data.clamp_scroll_offset(content_box_width, content_box_height);

    let layout_height = data.editor.try_layout().unwrap().height();
    if layout_height > content_box_height {
        assert!(
            data.scroll_offset > 0.0,
            "expected vertical scroll for overflowing multi-line input"
        );
    }
}

#[test]
fn scroll_by_clamps_and_bubbles() {
    let text = (0..40)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut data = make_input(true, &text);
    data.editor.set_width(Some(200.0));
    data.editor
        .driver(&mut FontContext::new(), &mut LayoutContext::new())
        .refresh_layout();

    let content_box_width = 200.0;
    let content_box_height = 30.0;
    let max = data.max_scroll_offset(content_box_width, content_box_height);
    assert!(max > 0.0, "test text should overflow the content box");

    // Scrolling up (positive delta decreases offset) while already at the top is a no-op and
    // the whole delta bubbles.
    assert_eq!(data.scroll_offset, 0.0);
    let bubbled = data.scroll_by(15.0, content_box_width, content_box_height);
    assert_eq!(data.scroll_offset, 0.0);
    assert_eq!(bubbled, 15.0);

    // Scrolling down moves the offset and consumes the delta.
    let bubbled = data.scroll_by(-10.0, content_box_width, content_box_height);
    assert_eq!(data.scroll_offset, 10.0);
    assert_eq!(bubbled, 0.0);

    // Scrolling past the end clamps to the maximum and bubbles the remainder. Starting at
    // offset 10 with max headroom of `max - 10`, a delta of `-(max + 100)` consumes
    // `max - 10` and bubbles the rest (`-110`).
    let bubbled = data.scroll_by(-(max + 100.0), content_box_width, content_box_height);
    assert_eq!(data.scroll_offset, max);
    assert!((bubbled - (-110.0)).abs() < 1e-3);
}

#[test]
fn single_line_does_not_scroll_when_text_fits() {
    let mut data = make_input(false, "hi");
    // Wide content box; nothing to scroll, so all delta bubbles.
    let bubbled = data.scroll_by(-50.0, 1000.0, 100.0);
    assert_eq!(data.scroll_offset, 0.0);
    assert_eq!(bubbled, -50.0);
}
