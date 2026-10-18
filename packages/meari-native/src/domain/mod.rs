use serde::Serialize;
pub mod graphics;
pub mod widgets;

pub use meari::domain::viewport::Viewport;

#[derive(Clone, Debug, Serialize)]
pub struct LayoutBox {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
