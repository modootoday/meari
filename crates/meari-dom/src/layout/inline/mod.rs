use blitz_traits::node_id::NodeId;
use parley::{AlignmentOptions, BreakReason, IndentOptions};
use style::values::specified::box_::{DisplayInside, DisplayOutside};
use style::values::{
    computed::{BaselineSource, CSSPixelLength, Contain},
    generics::text::GenericTextIndent,
};
use taffy::{
    AvailableSpace, AxisStaticPosition, BlockContainerStyle, BlockContext, BlockFormattingContext,
    BoxSizing, CollapsibleMarginSet, CompactLength, CoreStyle as _, Direction, LayoutInput,
    LayoutOutput, LayoutPartialTree as _, MaybeMath as _, MaybeResolve as _, OofCandidate,
    OofCandidates, OofItemStyle, OofPositioningArea, Overflow, Point, RequestedAxis,
    ResolveOrZero as _, RunMode, Size, SizingMode,
};

#[cfg(feature = "floats")]
use parley::YieldData;
#[cfg(feature = "floats")]
use taffy::{BlockItemStyle as _, Clear, Float, prelude::TaffyMaxContent};

use stylo_taffy::StyleFlags;

use super::resolve_calc_value;
use crate::layout::LayoutPassState;
use crate::stylo_to_parley;

#[cfg(test)]
mod layout_unit_tests {
    use super::layout_unit_ceil;

    #[test]
    fn rounds_up_to_one_sixty_fourth() {
        // Noto Sans 16px "2": 9.152px → Chrome 9.15625 (whole-pixel ceil gave 10).
        assert_eq!(layout_unit_ceil(9.152), 9.15625);
        assert_eq!(layout_unit_ceil(10.0), 10.0);
        assert_eq!(layout_unit_ceil(0.0), 0.0);
    }
}

mod inline_entry;
use inline_entry::*;
mod inline_layout_pass;
mod layout_unit;
pub(crate) use layout_unit::*;
mod inline_steps;
use inline_steps::*;
