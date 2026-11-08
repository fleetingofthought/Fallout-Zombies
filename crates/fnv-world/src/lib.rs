//! World data for Layer 3: worldspaces, cells, placed references and landscape,
//! resolved across a load order. No engine dependencies; the renderer consumes this.
//!
//! - [`alt_textures`]: records' alternate textures (MODS…): texture sets on named shapes
//! - [`audio`]: music types, media sets, acoustic spaces, audio markers and sound markers
//! - [`fields`]: schema-decoded access to one record's subrecords
//! - [`image_modifier`]: image space modifiers as animated screen effects
//! - [`imagespace`]: image spaces and the weather's adapters (the colour grade)
//! - [`imposter`]: references drawn at any distance while the player is in a region listing them
//! - [`index`]: which cells belong to which worldspace and which references to which cell
//! - [`placed`]: a placed reference (REFR/ACHR/ACRE/...) and the base object it places
//! - [`land`]: landscape heights, normals, vertex colours and texture layers
//! - [`grass`]: grass types and the plants generated on a landscape's textures
//! - [`lighting`]: interior cell lighting and light sources
//! - [`loadscreen`]: loading screens and the places they belong to
//! - [`sky`]: climate, weather and water records; time-of-day colours, sun and fog
//! - [`transform`]: reference rotation convention
//!
//! Format facts and confidence tags: `docs/spec-notes/world.md`.

pub mod alt_textures;
pub mod audio;
pub mod edit_plugin;
pub mod effect_shader;
pub mod fields;
pub mod grass;
pub mod image_modifier;
pub mod imagespace;
pub mod imposter;
pub mod index;
pub mod land;
pub mod lighting;
pub mod loadscreen;
pub mod placed;
pub mod radio;
pub mod sky;
pub mod transform;
pub mod xrgd;

pub use index::{Cell, RefEntry, WorldIndex, Worldspace};
pub use land::Land;
pub use placed::{BaseObject, Placement};

/// Exterior cell edge length in world units [documented: UESP; observed: every exterior
/// reference lies within 4096 units of its cell's grid square, see world.md].
pub const CELL_SIZE: f32 = 4096.0;

/// The exterior grid cell containing a world position.
pub fn cell_of(x: f32, y: f32) -> (i32, i32) {
    ((x / CELL_SIZE).floor() as i32, (y / CELL_SIZE).floor() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_cells() {
        assert_eq!(cell_of(0.0, 0.0), (0, 0));
        assert_eq!(cell_of(4095.9, 4096.0), (0, 1));
        assert_eq!(cell_of(-0.1, -4096.0), (-1, -1));
        assert_eq!(cell_of(-4096.1, 10.0), (-2, 0));
    }
}
