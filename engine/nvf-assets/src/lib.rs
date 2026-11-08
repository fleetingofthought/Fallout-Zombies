//! Bevy asset plugins shared by the viewer and the game: the game's archives (VFS), NIF
//! scene graphs and prepared static models, DDS textures, KF playback (one clip, or layered
//! clips blended by priority).

pub mod actor;
pub mod anim_blend;
pub mod anim_play;
pub mod camera_shot;
pub mod data;
pub mod guard;
pub mod material_anim;
pub mod model;
pub mod model_light;
pub mod morph;
pub mod particles;
pub mod read_pool;
pub mod release;
pub mod nif_material;
pub mod nif_spawn;
pub mod texture;

pub use data::{ArchivePlugin, GameData};
pub use particles::{ParticleSystemDef, PreparedParticles};
pub use model::{ModelCache, PreparedModel, PreparedShape};
pub use nif_material::{fade_prepass, nif_material, soft_edges, NifMaterial, NifMaterialPlugin};
pub use nif_spawn::{NifHidden, NifRoot, SpawnCtx, SpawnReport};
pub use release::{AssetReleasePlugin, KeepLimits};
pub use texture::TextureCache;
