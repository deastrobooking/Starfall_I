//! Public Heavy Water demo-game feature boundaries.
//!
//! This facade provides stable ownership paths while implementation files are
//! incrementally moved out of the historical `world` namespace. New game code
//! should import through this module instead of deep implementation paths.

/// Identity, progression, and save contracts shared across Heavy Water modes.
pub mod shared {
    pub use crate::world::heavy_water::*;
}

/// The bounded shared-screen platformer mode and its authored route language.
pub mod platformer {
    pub use crate::world::co_op_platformer::*;
    pub use crate::world::platformer_chunk_library::*;
    pub use crate::world::platformer_chunks::*;
    pub use crate::world::platformer_route_spawn::*;
    pub use crate::world::platformer_routes::*;
}
