use blitz_traits::node_id::NodeId;
#[cfg(feature = "svg")]
use markup5ever::local_name;

use crate::net::ResourceHandler;
use crate::node::{Node, NodeFlags};
use crate::tree::NodeTree;
use crate::{
    BaseDocument, net::ImageHandler, node::ImageResourceData, node::Status, util::ImageLayerKind,
};
use style::matching::StyleChange;
use style::properties::ComputedValues;
use style::properties::generated::longhands::position::computed_value::T as Position;
use style::selector_parser::{PseudoElement, RestyleDamage};
use style::shared_lock::StylesheetGuards;
use style::url::ComputedUrl;
use style::values::generics::image::Image as StyloImage;
use style::values::specified::align::AlignFlags;
use style::values::specified::box_::DisplayInside;
use style::values::specified::box_::DisplayOutside;
use thin_vec::ThinVec;

mod propagation;
pub(crate) use propagation::*;
mod style_images;
