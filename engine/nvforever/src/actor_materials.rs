//! Clips that drive the materials of actors' parts: the Securitron's Idle (`mtidle.kf`)
//! flickers its screen's glow, scales and rolls the face now and then and scrolls the
//! white noise over it; `Talking` and the screen-change idle flash the screen
//! (docs/spec-notes/effects-animation-audit.md §2). The tracks name the parts' shapes
//! (`Screen01:0`, `ScreenStatic:0`), which carry their name (`ShapeName`) and stored look
//! (`StoredLook`), as do the shapes of the weapon it holds (`weapons.rs`: the glowing
//! bulbs of energy weapons in `1hpattack3.kf`).
//!
//! Each frame the values come from the playing layers of the actor's `BlendPlayer`, the
//! strongest layer per member (`nvf_assets::material_anim::actor_material_values`); a
//! member no layer drives any more goes back to what the model stores [hypothesis: the
//! game may leave it where the clip left it; the idle's values sit at the stored ones
//! most of the time]. Only the members a track drives are written, so a texture swap
//! (`SetSecuritronExpression`, `texture_swap.rs`), SetActorAlpha or a fade keep theirs.
//!
//! A shape gets its own copy of the material it wears when first driven; when another
//! system gives it a new one (a swapped texture), that is copied in turn. A shape drawn
//! see-through (`Faded`, `Fading`) already wears a copy of its own, which is written as
//! it is.

use bevy::prelude::*;
use nvf_assets::actor::ShapeName;
use nvf_assets::anim_play::BlendPlayer;
use bevy::ecs::system::SystemParam;
use nvf_assets::anim_play::MaterialTarget;
use nvf_assets::material_anim::{actor_material_values, animator_material_values, pose_material, Driven, Look, StoredLook};
use std::collections::HashMap;
use nvf_assets::NifMaterial;
use nvf_play::fade::Fading;
use nvf_play::RefVisual;

use crate::effect_shaders::Faded;

/// Frames between looks for named shapes that appeared on an actor since.
const RESCAN_FRAMES: u32 = 60;

/// On an actor whose clips drive materials of its shapes.
#[derive(Component, Default)]
pub struct ActorMaterials {
    slots: Vec<Slot>,
    /// Shape names the clips have named so far (lower case).
    searched: Vec<String>,
}

impl ActorMaterials {
    /// The shapes the clips drive and the copy of its own each was given, if any (the
    /// profile aid's `profile change sources:` line). Posing writes the material a shape
    /// wears: its own copy, or, while it fades or is drawn faded, the see-through copy it
    /// wears then (`Posing::pose`), so the profile reads each shape's worn material too.
    pub fn shapes(&self) -> impl Iterator<Item = (Entity, Option<AssetId<NifMaterial>>)> + '_ {
        self.slots.iter().map(|s| (s.entity, s.mine.as_ref().map(Handle::id)))
    }

    /// One driven shape, nothing written yet (tests).
    #[cfg(test)]
    pub(crate) fn with_shape(entity: Entity) -> Self {
        ActorMaterials { slots: vec![Slot { shape: String::new(), entity, mine: None, last: None }], searched: Vec::new() }
    }
}

/// The driven materials of the player's rigs (the body, the first-person arms with the
/// weapon in hand), by rig root: `pose_player`'s, kept as a resource so the profile aid
/// can see them.
#[derive(Resource, Default)]
pub struct PlayerRigMaterials(pub(crate) HashMap<Entity, ActorMaterials>);

struct Slot {
    /// Lower case, as `ShapeName`.
    shape: String,
    entity: Entity,
    /// The copy of the material this shape wears that is written.
    mine: Option<Handle<NifMaterial>>,
    /// What was written last, and which members.
    last: Option<(Look, Driven)>,
}

/// Find the drawn shapes named `name` (lower case) below `root`.
fn find(root: Entity, name: &str, children: &Query<&Children>, names: &Query<&ShapeName>, out: &mut Vec<Entity>) {
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if names.get(e).is_ok_and(|n| n.0 == name) {
            out.push(e);
        }
        if let Ok(c) = children.get(e) {
            stack.extend(c.iter());
        }
    }
}

/// A driven shape: what it wears, what its model stores, and whether it is drawn
/// see-through (wearing a copy of its own already).
type DrivenShape = (&'static MeshMaterial3d<NifMaterial>, &'static StoredLook, Has<Fading>, Has<Faded>);

/// What posing the driven materials of an actor needs.
#[derive(SystemParam)]
pub struct Posing<'w, 's> {
    commands: Commands<'w, 's>,
    children: Query<'w, 's, &'static Children>,
    names: Query<'w, 's, &'static ShapeName>,
    shapes: Query<'w, 's, DrivenShape>,
    materials: ResMut<'w, Assets<NifMaterial>>,
    refs: Query<'w, 's, &'static RefVisual>,
    found: Local<'s, Vec<Entity>>,
    frame: Local<'s, u32>,
}

impl Posing<'_, '_> {
    /// Write `values` (from [`actor_material_values`]) into the shapes below `actor`
    /// that `am` tracks, finding newly named ones (every frame for names not found yet,
    /// now and then for all: a weapon drawn or put on since).
    fn pose(&mut self, actor: Entity, am: &mut ActorMaterials, values: &[(&str, MaterialTarget, [f32; 3], f32)], look_again: bool) {
        for (shape, ..) in values {
            let shape = shape.to_ascii_lowercase();
            if !am.searched.contains(&shape) {
                am.searched.push(shape);
            }
        }
        for k in 0..am.searched.len() {
            let shape = &am.searched[k];
            if !look_again && am.slots.iter().any(|s| &s.shape == shape) {
                continue;
            }
            self.found.clear();
            find(actor, shape, &self.children, &self.names, &mut self.found);
            for &entity in self.found.iter() {
                if !am.slots.iter().any(|s| s.entity == entity) {
                    am.slots.push(Slot { shape: shape.clone(), entity, mine: None, last: None });
                    let r = self.refs.get(actor).map_or(0, |r| r.0);
                    info!("actor materials: reference {r:08X} shape {shape} driven by its clips");
                }
            }
        }
        let mut gone = false;
        for slot in &mut am.slots {
            let Ok((current, stored, fading, faded)) = self.shapes.get(slot.entity) else {
                gone = true;
                continue;
            };
            let mut look = stored.0;
            let mut driven = Driven::default();
            for (_, target, v, _) in values.iter().filter(|(s, ..)| s.eq_ignore_ascii_case(&slot.shape)) {
                look.take(*target, *v);
                driven = driven.or(Driven::of(*target));
            }
            let write = match slot.last {
                None if !driven.any() => continue,
                Some(last) if last == (look, driven) => continue,
                Some((_, before)) => driven.or(before),
                None => driven,
            };
            slot.last = driven.any().then_some((look, driven));
            if fading || faded || slot.mine.as_ref().is_some_and(|m| m.id() == current.id()) {
                if let Some(mut m) = self.materials.get_mut(&current.0) {
                    pose_material(&mut m, &look, write, true);
                }
            } else if let Some(mut m) = self.materials.get(&current.0).cloned() {
                // Its own copy of what it wears now (the model's, or a swapped texture's).
                pose_material(&mut m, &look, write, true);
                let h = self.materials.add(m);
                self.commands.entity(slot.entity).insert(MeshMaterial3d(h.clone()));
                slot.mine = Some(h);
            }
        }
        if gone {
            // A part or weapon was taken away.
            let shapes = &self.shapes;
            am.slots.retain(|s| shapes.contains(s.entity));
        }
    }

    /// Count a frame; whether this one looks again for every named shape.
    fn tick(&mut self) -> bool {
        *self.frame = self.frame.wrapping_add(1);
        self.frame.is_multiple_of(RESCAN_FRAMES)
    }
}

/// Pose the driven materials of every animated actor (after its skeleton is posed).
pub fn pose(mut actors: Query<(Entity, &BlendPlayer, Option<&mut ActorMaterials>)>, mut posing: Posing) {
    let look_again = posing.tick();
    for (actor, player, am) in &mut actors {
        if am.is_none() && player.layers.iter().flatten().all(|l| l.weight <= 0.0 || l.clip.clip.materials.is_empty()) {
            continue;
        }
        let mut values = Vec::new();
        actor_material_values(player, &mut values);
        match am {
            Some(mut am) => posing.pose(actor, &mut am, &values, look_again),
            None if values.is_empty() => {}
            None => {
                let mut fresh = ActorMaterials::default();
                posing.pose(actor, &mut fresh, &values, look_again);
                posing.commands.entity(actor).insert(fresh);
            }
        }
    }
}

/// The same for the player's rigs (the body, and the first-person arms with the weapon
/// in hand), whose clips an `Animator` plays (`player_body.rs`).
pub fn pose_player(body: Option<Res<crate::player_body::PlayerBody>>, mut posing: Posing, mut kept: ResMut<PlayerRigMaterials>) {
    let Some(body) = body else { return };
    let kept = &mut kept.0;
    let look_again = posing.tick();
    let rigs = body.animators();
    kept.retain(|e, _| rigs.iter().any(|(r, _)| r == e));
    for (root, animator) in rigs {
        let mut values = Vec::new();
        animator_material_values(animator, &mut values);
        if values.is_empty() && !kept.contains_key(&root) {
            continue;
        }
        let am = kept.entry(root).or_default();
        posing.pose(root, am, &values, look_again);
    }
}
