//! MarkIts — Semantic Annotation SVG Engine
//!
//! A lightweight Rust library that generates high-quality SVG annotation overlays
//! from semantic JSON instructions.

pub mod error;
pub mod layout;
pub mod model;
pub mod renderer;
pub mod semantic;
pub mod theme;

pub use error::{MarkitsError, Result};
pub use layout::{LayoutEngine, ResolvedAnnotation, ResolvedScene};
pub use model::{Annotation, Canvas, PositionHint, Scene, SemanticStyle, TargetRect};
pub use renderer::{render_debug_from_json, render_from_json, render_with_layout_from_json, LayoutElement, RenderResult, SvgRenderer};
pub use theme::Theme;
