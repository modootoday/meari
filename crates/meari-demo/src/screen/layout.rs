//! Pure geometry: split the screen into two non-overlapping panes (native left, web right).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub struct Panes {
    pub native: Rect,
    pub web: Rect,
}

const MARGIN: u32 = 16;
/// Space for window decorations and the desktop panel.
const TOP: u32 = 48;
const BOTTOM: u32 = 64;
const MAX_WIDTH: u32 = 900;
const MAX_HEIGHT: u32 = 900;

pub fn split(screen_width: u32, screen_height: u32) -> Panes {
    let width = ((screen_width.saturating_sub(3 * MARGIN)) / 2).clamp(320, MAX_WIDTH);
    let height = screen_height
        .saturating_sub(TOP + BOTTOM)
        .clamp(240, MAX_HEIGHT);
    let used = 2 * width + MARGIN;
    let left = (screen_width.saturating_sub(used) / 2) as i32;
    let pane = |x: i32| Rect {
        x,
        y: TOP as i32,
        width,
        height,
    };
    Panes {
        native: pane(left),
        web: pane(left + (width + MARGIN) as i32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_do_not_overlap_on_small_screen() {
        let p = split(1280, 800);
        assert!(p.native.x + p.native.width as i32 <= p.web.x);
        assert!(p.web.x + p.web.width as i32 <= 1280);
        assert!(p.native.y + p.native.height as i32 <= 800);
    }
}
