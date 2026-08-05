//! Find and prepare raid-frame text without loading an OCR model.

pub mod bands;
pub mod preprocess;
pub mod text;

pub use bands::{Band, BandKind, detect_bands, harmonize};
pub use preprocess::{PreparedCrop, prepare};
pub use text::parse_health_text;
