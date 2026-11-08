//! Effect shaders on references (EFSH, docs/spec-notes/effect-shaders.md) and actors'
//! alpha (`SetActorAlpha`).
//!
//! - **What plays**: the shaders scripts play (`PlayMagicShaderVisuals`,
//!   `fnv_game::functions::misc::ScriptLooks`) and those of the magic effects active on
//!   actors (MGEF "effect shader", for as long as the effect lasts).
//! - **Drawing**: the membrane only (§4 of the notes). Every mesh drawn for the reference
//!   gets up to two overlays drawn on the same surface (its mesh, and its skin when it
//!   has one): the fill (texture × colour, panned by the texture speed) and the edge
//!   (the edge colour, faded in towards the outline by the NIF material's view-angle
//!   falloff). Blended meshes (beards, lashes) get none: the overlays do not carry the
//!   mesh's texture alpha; nor do refracting ones (heat haze), which show no surface. Alpha-tested meshes (a robot's body, cut-out hair) get copies
//!   cut by their own texture's alpha at their own cut (the holes test turned round,
//!   `nvf_assets::nif_material` "Holes"; none when the shader has holes of its own),
//!   #1717. The player's are the meshes of its
//!   third-person body. Both are unlit, cast no shadow and sit a decal's depth step in
//!   front of the surface. Their alpha follows the record's cycle every frame; a mesh drawn later
//!   (a cell loaded again, armour put on) gets its overlays then.
//! - **Holes** (§7 of the notes): a shader with a holes texture gives every mesh it
//!   covers (and its own overlays) a copy of the mesh's material that the holes texture
//!   eats away as the record's value moves (`nvf_assets::nif_material`, "Holes"), the
//!   band next to the holes in the holes' edge colour; an opaque mesh's copy is drawn
//!   alpha-tested so the depth passes cut the same holes. When the play ends the mesh
//!   gets its own material back.
//! - **Add-on models** (§8): a shader naming a DEBR puts one of its models (by the
//!   models' percentages) on up to [`ADDON_PLACES`] bones of the reference's skinned
//!   meshes, spread over them (an unskinned reference: one, at its first mesh), each
//!   played as an effect model (`crate::shot_fx::spawn_attached`: billboards turn to the
//!   camera, its clip plays), grown and faded in from the start and shrunk and faded out
//!   after the end by the record's add-on timing.
//! - **Ending**: a script's play with a duration ends after max(duration, one cycle),
//!   without one when stopped; an effect's when the effect ends.
//! - **Actor alpha**: the meshes of an actor with an alpha below 1 get blended copies of
//!   their materials with the alpha multiplied in; 1 gives the originals back.
//!
//! Log: `effect shader: <EDID> on <who> (…)` when one starts, `… ends (…)` when it ends,
//! `effect shader: <EDID> holes on <n> meshes of <who> (…)`, `effect shader: <EDID> puts
//! <n> add-on models of DEBR <id> on <who> (…)`, `actor alpha: <who> <a> (<n> meshes)`,
//! and at exit `effect shaders summary: …`.

use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::prelude::*;
use fnv_game::data::MagicEffectDef;
use fnv_sim::FormId;
use fnv_world::effect_shader::{blend, Addons, EffectShader, Holes};
use nvf_assets::material_anim::RigMaterials;
use nvf_assets::model::ModelCtx;
use nvf_assets::nif_material::nif_material;
use nvf_assets::NifMaterial;
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::collections::{HashMap, HashSet};

use crate::world::WorldData;

pub struct EffectShadersPlugin;

impl Plugin for EffectShadersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shaders>()
            .init_resource::<crate::effect_particles::EffectParticles>()
            .add_systems(Update, (play.after(nvf_play::player::BodySet), sync_player_effects.after(play), actor_alpha.after(crate::texture_swap::apply), crate::effect_particles::draw.after(sync_player_effects), addons_leaving.after(play)))
            .add_systems(Last, (report, crate::effect_particles::report, release_faded));
    }
}

/// What a play comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Source {
    /// A script's play (its serial).
    Script(u64),
    /// An active magic effect (its id).
    Effect(u32),
    /// A companion perk's mark while the player aims (`nvf_play::perk_marks`): the
    /// reference and the shader.
    Perk(u32, u32),
}

/// An overlay's fill and edge materials.
type Looks = [Option<Handle<NifMaterial>>; 2];

/// A shader playing on a reference.
pub(crate) struct Active {
    pub(crate) on: u32,
    pub(crate) shader: Box<EffectShader>,
    /// `Time::elapsed_secs` when it started.
    pub(crate) since: f32,
    /// Ends at this many seconds from `since`; `None`: until stopped.
    until: Option<f32>,
    /// The persistent part shows after the cycle.
    pub(crate) persist: bool,
    fill: Option<Handle<NifMaterial>>,
    edge: Option<Handle<NifMaterial>>,
    /// Copies of `fill` and `edge` cut by an alpha-tested mesh's own texture, by that
    /// texture and cut (in alpha units): see [`cut_out_overlay`].
    cut: HashMap<(AssetId<Image>, u8), Looks>,
    /// The meshes covered, and their overlays.
    pub(crate) covered: HashMap<Entity, Vec<Entity>>,
    /// [design: sync_player_effects] Last reported camera-visible and total meshes.
    camera_sees: Option<(usize, usize)>,
    /// The holes texture, when the shader has holes and it was found.
    holes_texture: Option<Handle<Image>>,
    /// Meshes given a material with holes: their own material and the copy.
    holed: HashMap<Entity, (Handle<NifMaterial>, Handle<NifMaterial>)>,
    /// The add-on models put on the reference: (entity above the model, rig root that
    /// fades it); empty until the reference is drawn.
    addons: Vec<(Entity, Option<Entity>)>,
}

/// How many bones of a reference get an add-on model at most [design: the record does
/// not say how many; effect-shaders.md §8].
pub const ADDON_PLACES: usize = 8;

/// Add-on models shrinking and fading after their play ended.
struct Leaving {
    addons: Addons,
    models: Vec<(Entity, Option<Entity>)>,
    /// `Time::elapsed_secs` of the play's start and end.
    since: f32,
    ended: f32,
}

/// Put on an overlay mesh (not covered itself, not faded by actor alpha).
#[derive(Component)]
pub(crate) struct Overlay;

/// [design: sync_player_effects] Only player overlays need to follow view changes.
#[derive(Component)]
pub(crate) struct PlayerOverlay(Entity);

/// [design: player_body::Seen] A player's add-on holder follows this body part.
#[derive(Component)]
pub(crate) struct PlayerAddon(crate::player_body::EffectPart);

/// [design: particles::emit] Check the part directly, even before visibility propagates.
#[derive(Component)]
pub(crate) struct PlayerEffectEmitter(pub(crate) crate::player_body::EffectPart);

fn overlay_layers(mesh: Option<&RenderLayers>) -> RenderLayers {
    mesh.cloned().unwrap_or_default()
}

pub(crate) fn player_mesh_seen(mesh: Option<&RenderLayers>) -> bool {
    use crate::sky::{ARMS_LAYER, BODY_LAYER};
    overlay_layers(mesh).intersects(&RenderLayers::from_layers(&[0, ARMS_LAYER, BODY_LAYER]))
}

/// [design: player_body::show_rigs] Render layers do not propagate to children.
pub(crate) fn sync_player_effects(
    mut st: ResMut<Shaders>,
    body: Option<Res<crate::player_body::PlayerBody>>,
    meshes: Query<&RenderLayers, Without<Overlay>>,
    shown: Query<&InheritedVisibility>,
    mut overlays: Query<(&PlayerOverlay, &mut RenderLayers), With<Overlay>>,
    mut addons: Query<(&PlayerAddon, &mut Visibility)>,
) {
    for (parent, mut layer) in &mut overlays {
        let target = overlay_layers(meshes.get(parent.0).ok());
        if *layer != target {
            *layer = target;
        }
    }
    for (part, mut visibility) in &mut addons {
        let target = if body.as_deref().is_some_and(|b| b.sees_effect(part.0)) { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != target {
            *visibility = target;
        }
    }
    for a in st.active.values_mut().filter(|a| a.on == FormId::PLAYER_REF.0) {
        let seen = a.covered.keys().filter(|e| player_mesh_seen(meshes.get(**e).ok()) && !shown.get(**e).is_ok_and(|v| !v.get())).count();
        let counts = (seen, a.covered.len());
        if a.camera_sees != Some(counts) {
            info!("effect shader: {} on the player: the camera sees {} of {} covered meshes", a.shader.edid, counts.0, counts.1);
            a.camera_sees = Some(counts);
        }
    }
}

/// [design: shot_fx::spawn_attached] Mark only this player's add-on emitters once,
/// after the commands that create the model's node tree have been applied.
fn player_addon(commands: &mut Commands, holder: Entity, part: crate::player_body::EffectPart, seen: bool) {
    commands.entity(holder).insert((PlayerAddon(part), if seen { Visibility::Inherited } else { Visibility::Hidden }));
    commands.queue(move |world: &mut World| {
        let mut pending = vec![holder];
        while let Some(e) = pending.pop() {
            if let Some(children) = world.get::<Children>(e) {
                pending.extend(children.iter());
            }
            if world.get::<crate::particles::AddonEmitter>(e).is_some() || world.get::<crate::particles::ModelEmitter>(e).is_some() {
                world.entity_mut(e).insert(PlayerEffectEmitter(part));
            }
        }
    });
}

/// On a mesh drawn see-through for its actor's alpha: its own material and the alpha.
/// Another system that changes the mesh's material meanwhile (`crate::texture_swap`)
/// changes `original`, which comes back when the alpha does.
#[derive(Component)]
pub(crate) struct Faded {
    pub(crate) original: Handle<NifMaterial>,
    alpha: f32,
    /// The material the copy was made from (`original` when nothing changed it since).
    made_from: AssetId<NifMaterial>,
}

#[derive(Resource, Default)]
pub(crate) struct Shaders {
    records: HashMap<u32, Option<Box<EffectShader>>>,
    pub(crate) active: HashMap<Source, Active>,
    /// Blended copies of materials for actor alpha: (original, alpha in 1/256) → copy.
    faded: HashMap<(AssetId<NifMaterial>, u16), Handle<NifMaterial>>,
    leaving: Vec<Leaving>,
    played: u32,
    holed: u32,
    addons: u32,
    ended: u32,
    overlays: u32,
    alpha_calls: u32,
}

fn linear(c: [u8; 3]) -> LinearRgba {
    Color::srgb_u8(c[0], c[1], c[2]).to_linear()
}

/// How a membrane blends onto what lies behind it, from its destination blend (notes §4).
fn alpha_mode(s: &EffectShader) -> AlphaMode {
    match s.dest_blend {
        blend::SOURCE_INVERTED_ALPHA => AlphaMode::Blend,
        blend::DEST_COLOR => AlphaMode::Multiply,
        blend::ONE | blend::SOURCE_ALPHA => AlphaMode::Add,
        _ => AlphaMode::Add,
    }
}

/// The view-angle falloff of the edge: none facing the camera, half at cos θ =
/// 1 − 0.5^(1/fall off), a linear ramp through it (notes §4, design).
fn edge_falloff(fall_off: f32) -> Vec4 {
    let f = if fall_off.is_finite() && fall_off > 0.01 { fall_off } else { 1.0 };
    let half = 1.0 - 0.5f32.powf(1.0 / f);
    Vec4::new(1.0, 2.0 * half - 1.0, 0.0, 1.0)
}

fn overlay_material(s: &EffectShader, color: [u8; 3], texture: Option<Handle<Image>>, falloff: Option<Vec4>) -> NifMaterial {
    let mut m = nif_material(StandardMaterial {
        base_color: Color::from(linear(color)).with_alpha(0.0),
        base_color_texture: texture,
        unlit: true,
        alpha_mode: alpha_mode(s),
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    m.extension.decal = true;
    m.extension.hard_edges = true;
    if let Some(f) = falloff {
        m.extension.falloff = f;
    }
    m
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play(
    mut commands: Commands,
    time: Res<Time>,
    mut script: Option<ResMut<ScriptState>>,
    world: Option<Res<WorldData>>,
    mut models: ModelCtx,
    mut st: ResMut<Shaders>,
    drawn: Query<(Entity, &RefVisual, &Mesh3d, Option<&SkinnedMesh>, &MeshMaterial3d<NifMaterial>), Without<Overlay>>,
    layers: Query<&RenderLayers>,
    (body, children, body_meshes): (
        Option<Res<crate::player_body::PlayerBody>>,
        Query<&Children>,
        Query<(&Mesh3d, Option<&SkinnedMesh>, &MeshMaterial3d<NifMaterial>), (Without<Overlay>, Without<RefVisual>)>,
    ),
    (combat, mut fades, mut transforms, perk_marks): (Option<Res<nvf_play::combat::Combat>>, Query<&mut RigMaterials>, Query<&mut Transform>, Option<Res<nvf_play::perk_marks::PerkMarks>>),
) {
    let (Some(rt), Some(world)) = (script.as_deref_mut().and_then(|s| s.rt.as_mut()), world) else { return };
    let st = &mut *st;
    let now = time.elapsed_secs();
    let game = &mut rt.game;

    // What should play: (source, reference, shader, duration, persist).
    let mut want: Vec<(Source, u32, u32, Option<f64>, bool)> =
        game.script_looks.shaders.iter().map(|p| (Source::Script(p.serial), p.on.0, p.shader.0, p.duration, true)).collect();
    for e in &game.effects {
        let Some(m) = game.data.magic_effects.get(&e.effect) else { continue };
        if let Some(sh) = m.shader {
            want.push((Source::Effect(e.id), e.target.0, sh.0, None, m.has(MagicEffectDef::FX_PERSIST)));
        }
    }
    if let Some(marks) = perk_marks.as_deref() {
        want.extend(marks.0.iter().map(|(r, sh, _)| (Source::Perk(r.0, sh.0), r.0, sh.0, None, true)));
    }
    let wanted: HashSet<Source> = want.iter().map(|w| w.0).collect();

    // Ends: stopped, the effect over, or its time up.
    let mut ended: Vec<(Source, &'static str)> = st.active.keys().filter(|k| !wanted.contains(k)).map(|k| (*k, "stopped")).collect();
    for (k, a) in &st.active {
        if a.until.is_some_and(|u| now - a.since >= u) {
            ended.push((*k, "its time is up"));
        }
    }
    let mut gone: HashSet<Source> = HashSet::new();
    for (k, why) in ended {
        let Some(a) = st.active.remove(&k) else { continue };
        gone.insert(k);
        for x in a.covered.values().flatten() {
            commands.entity(*x).try_despawn();
        }
        for h in [a.fill, a.edge].into_iter().chain(a.cut.into_values().flatten()).flatten() {
            models.materials.remove(&h);
        }
        // The meshes get their own materials back, unless something else changed them since.
        for (e, (original, copy)) in a.holed {
            if drawn.get(e).map(|d| d.4 .0 == copy).or_else(|_| body_meshes.get(e).map(|d| d.2 .0 == copy)).unwrap_or(false) {
                commands.entity(e).try_insert(MeshMaterial3d(original));
            }
            models.materials.remove(&copy);
        }
        if let Some(addons) = a.shader.addons.filter(|x| x.linger() > 0.0 && !a.addons.is_empty()) {
            st.leaving.push(Leaving { addons, models: a.addons, since: a.since, ended: now });
        } else {
            for (holder, _) in a.addons {
                commands.entity(holder).try_despawn();
            }
        }
        if let Source::Script(serial) = k {
            game.script_looks.end(serial);
        }
        st.ended += 1;
        info!("effect shader: {} on {} ends ({why})", a.shader.edid, game.data.name(FormId(a.on)));
    }

    // Starts.
    for (k, on, shader, duration, persist) in want {
        if st.active.contains_key(&k) || gone.contains(&k) {
            continue;
        }
        let record = st.records.entry(shader).or_insert_with(|| fnv_world::effect_shader::effect_shader(world.lo, world.schema, shader).map(Box::new)).clone();
        let Some(s) = record else {
            // Not an EFSH: nothing to draw; a script's play is let go.
            if let Source::Script(serial) = k {
                warn!("effect shader: {shader:08X} is not an effect shader");
                game.script_looks.end(serial);
            }
            continue;
        };
        let mut load = |what: &str, t: &str| {
            let path = texture_path(t);
            let a = &mut models;
            let image = a.textures.get(&path, true, &a.data, &mut a.images, a.formats.as_deref());
            if image.is_none() {
                warn!("effect shader: {}: {what} texture {path} not found", s.edid);
            }
            image
        };
        let texture = s.fill_texture.as_deref().and_then(|t| load("fill", t));
        let holes_texture = s.holes.as_ref().and_then(|h| load("holes", &h.texture));
        let shows = |c: &fnv_world::effect_shader::AlphaCycle| c.full > 0.0 || (persist && c.persistent > 0.0);
        let with_holes = |mut m: NifMaterial| {
            if let (Some(h), Some(tex)) = (&s.holes, &holes_texture) {
                set_holes(&mut m, h, tex.clone(), h.value(0.0));
            }
            m
        };
        let fill = (s.has_membrane() && shows(&s.fill)).then(|| models.materials.add(with_holes(overlay_material(&s, s.fill_color, texture, None))));
        let edge = (s.has_membrane() && shows(&s.edge)).then(|| models.materials.add(with_holes(overlay_material(&s, s.edge_color, None, Some(edge_falloff(s.edge_fall_off))))));
        let until = duration.map(|d| (d as f32).max(s.cycle()));
        st.played += 1;
        info!(
            "effect shader: {} on {} ({}{}, {})",
            s.edid,
            game.data.name(FormId(on)),
            if fill.is_some() { "fill" } else { "no fill" },
            if edge.is_some() { " and edge" } else { "" },
            match (until, k) {
                (Some(u), _) => format!("for {u:.1} s"),
                (None, Source::Script(_)) => "until stopped".to_string(),
                (None, Source::Effect(_)) => "while the effect lasts".to_string(),
                (None, Source::Perk(..)) => "while the player aims (a companion perk)".to_string(),
            }
        );
        st.active.insert(k, Active { on, shader: s, since: now, until, persist, fill, edge, cut: HashMap::new(), covered: HashMap::new(), camera_sees: None, holes_texture, holed: HashMap::new(), addons: Vec::new() });
    }

    // [documented: #1323] Ends and starts must run even when no shader remains to draw.
    if st.active.is_empty() {
        return;
    }

    // Cover only the references targeted by active shaders; animate the materials.
    type Drawn<'a> = (Entity, &'a Mesh3d, Option<&'a SkinnedMesh>, &'a MeshMaterial3d<NifMaterial>);
    let mut by_ref: HashMap<u32, Vec<Drawn<'_>>> = st.active.values().map(|a| (a.on, Vec::new())).collect();
    for (e, rv, mesh, skin, mat) in &drawn {
        if let Some(list) = by_ref.get_mut(&rv.0) {
            list.push((e, mesh, skin, mat));
        }
    }
    // [design: player_body::show_rigs] The third-person body has no RefVisual. Its
    // overlays copy its layers; attached add-on models are not the body's own meshes.
    if let Some((list, root)) = by_ref.get_mut(&FormId::PLAYER_REF.0).and_then(|list| {
        body.as_deref().and_then(|b| b.roots().first().copied()).map(|root| (list, root))
    }) {
        let addon_parts: HashSet<Entity> = st.active.values().flat_map(|a| a.addons.iter().map(|x| x.0)).chain(st.leaving.iter().flat_map(|l| l.models.iter().map(|x| x.0))).flat_map(|h| std::iter::once(h).chain(children.iter_descendants(h))).collect();
        for e in children.iter_descendants(root) {
            if addon_parts.contains(&e) {
                continue;
            }
            if let Ok((mesh, skin, mat)) = body_meshes.get(e) {
                list.push((e, mesh, skin, mat));
            }
        }
    }
    let (holed_count, addon_count) = (&mut st.holed, &mut st.addons);
    for a in st.active.values_mut() {
        let meshes = by_ref.get(&a.on).map(Vec::as_slice).unwrap_or(&[]);
        a.covered.retain(|e, _| meshes.iter().any(|(m, ..)| m == e));
        a.holed.retain(|e, _| meshes.iter().any(|(m, ..)| m == e));
        let before = a.covered.len();
        for &(e, mesh, skin, mat) in meshes {
            if a.covered.contains_key(&e) {
                continue;
            }
            // A blended mesh (beards, lashes) gets no overlay: drawn without the mesh's
            // texture alpha, it covered every card of the hair in the membrane's colour
            // and the head showed bald (owner's playtest, 2026-10-24). An alpha-tested
            // one (a robot's body, #1717) gets overlays cut by its own texture.
            let own = a.holed.get(&e).map_or(&mat.0, |(original, _)| original);
            let look = match models.materials.get(own).map(|m| cut_out_overlay(m, a.shader.holes.is_some())) {
                Some(Cover::Plain) | None => [a.fill.clone(), a.edge.clone()],
                Some(Cover::CutBy(tex, cut)) => {
                    let key = (tex.id(), (cut * 255.0).round() as u8);
                    a.cut
                        .entry(key)
                        .or_insert_with(|| {
                            [&a.fill, &a.edge].map(|m| {
                                let copy = models.materials.get(m.as_ref()?)?.clone();
                                Some(models.materials.add(cut_by(copy, tex.clone(), cut)))
                            })
                        })
                        .clone()
                }
                Some(Cover::None) => {
                    a.covered.insert(e, Vec::new());
                    continue;
                }
            };
            let mut overlays = Vec::new();
            for mat in look.iter().flatten() {
                let mut c = commands.spawn((Mesh3d(mesh.0.clone()), MeshMaterial3d(mat.clone()), Transform::IDENTITY, Overlay, overlay_layers(layers.get(e).ok()), NoFrustumCulling, bevy::light::NotShadowCaster));
                if a.on == FormId::PLAYER_REF.0 {
                    c.insert(PlayerOverlay(e));
                }
                if let Some(skin) = skin {
                    c.insert(skin.clone());
                }
                let x = c.id();
                commands.entity(e).add_child(x);
                overlays.push(x);
            }
            st.overlays += overlays.len() as u32;
            a.covered.insert(e, overlays);
        }
        if a.covered.len() != before && before == 0 && !a.covered.is_empty() {
            info!("effect shader: {} covers {} meshes of {}", a.shader.edid, a.covered.len(), game.data.name(FormId(a.on)));
        }
        let t = now - a.since;
        // Holes: each mesh drawn with a copy of its material that the holes eat away.
        let holes = if let (Some(h), Some(tex)) = (a.shader.holes.as_ref(), a.holes_texture.as_ref()) {
            let value = h.value(t);
            let first = a.holed.is_empty();
            for &(e, _, _, mat) in meshes {
                let current = &mat.0;
                if a.holed.get(&e).is_some_and(|(_, copy)| copy == current) {
                    continue;
                }
                // New, or something else gave it another material since: copy that one.
                let Some(own) = models.materials.get(current).cloned() else { continue };
                let mut m = own;
                set_holes(&mut m, h, tex.clone(), value);
                if matches!(m.base.alpha_mode, AlphaMode::Opaque) {
                    m.base.alpha_mode = AlphaMode::Mask(0.0);
                    m.extension.hard_edges = true;
                }
                let copy = models.materials.add(m);
                commands.entity(e).try_insert(MeshMaterial3d(copy.clone()));
                if let Some((_, old)) = a.holed.insert(e, (current.clone(), copy)) {
                    models.materials.remove(&old);
                }
                *holed_count += 1;
            }
            if first && !a.holed.is_empty() {
                info!(
                    "effect shader: {} holes on {} meshes of {} ({}, {:.0} to {:.0} from {:.2} to {:.2} s, edge {:.1} units of ({}, {}, {}))",
                    a.shader.edid,
                    a.holed.len(),
                    game.data.name(FormId(a.on)),
                    texture_path(&h.texture),
                    h.start_value,
                    h.end_value,
                    h.start_time,
                    h.end_time,
                    h.edge_width,
                    h.edge_color[0],
                    h.edge_color[1],
                    h.edge_color[2]
                );
            }
            for (_, copy) in a.holed.values() {
                animate_material(&mut models.materials, copy, Some(value), None, None);
            }
            Some(value)
        } else {
            None
        };
        // Add-on models: put on once the reference is drawn, then grown and faded in.
        if let Some(ad) = a.shader.addons {
            if a.addons.is_empty() && !meshes.is_empty() {
                let mut bones: Vec<Entity> = meshes.iter().filter_map(|m| m.2).flat_map(|s| s.joints.iter().copied()).collect();
                bones.sort();
                bones.dedup();
                let places: Vec<Entity> = if bones.is_empty() { vec![meshes[0].0] } else { spread(&bones, ADDON_PLACES) };
                let choices: Vec<(u8, String)> = combat.as_deref().and_then(|c| c.forms.debris.get(&FormId(ad.debris))).map(|m| m.iter().map(|m| (m.percent, m.model.clone())).collect()).unwrap_or_default();
                let total: u32 = choices.iter().map(|c| c.0 as u32).sum();
                let mut first_model = String::new();
                for (i, &place) in places.iter().enumerate() {
                    if total == 0 {
                        break;
                    }
                    // A fixed pick per place and play [design].
                    let mut pick = (mix(a.on as u64 ^ ((a.since.to_bits() as u64) << 20) ^ i as u64) % total as u64) as i64;
                    let Some((_, model)) = choices.iter().find(|c| {
                        pick -= c.0 as i64;
                        pick < 0
                    }) else {
                        continue;
                    };
                    let path = fnv_actor::resolve::mesh_path(model);
                    let Some(pm) = models.model(&path) else { continue };
                    let (scale, _) = ad.look(t, None);
                    let made = crate::shot_fx::spawn_attached(&mut commands, place, &pm, Transform::from_scale(Vec3::splat(scale)), &path);
                    if a.on == FormId::PLAYER_REF.0 {
                        let part = body.as_deref().map_or(crate::player_body::EffectPart::Body, |b| b.effect_part(place));
                        let seen = body.as_deref().is_some_and(|b| b.sees_effect(part));
                        player_addon(&mut commands, made.0, part, seen);
                    }
                    if first_model.is_empty() {
                        first_model = path;
                    }
                    a.addons.push(made);
                }
                if a.addons.is_empty() {
                    // Nothing to put on (no DEBR, or none of its models loads): do not try again.
                    a.shader.addons = None;
                    warn!("effect shader: {}: no add-on model of DEBR {:08X} could be drawn", a.shader.edid, ad.debris);
                } else {
                    *addon_count += a.addons.len() as u32;
                    info!(
                        "effect shader: {} puts {} add-on models of DEBR {:08X} on {} ({} first; fade in {:.2} s, out {:.2} s, scale {:.2} to 1 over {:.2} s, to {:.2} over {:.2} s at the end)",
                        a.shader.edid,
                        a.addons.len(),
                        ad.debris,
                        game.data.name(FormId(a.on)),
                        first_model,
                        ad.fade_in,
                        ad.fade_out,
                        ad.scale_start,
                        ad.scale_in,
                        ad.scale_end,
                        ad.scale_out
                    );
                }
            }
            let (scale, alpha) = ad.look(t, None);
            addon_look(&a.addons, scale, alpha, &mut transforms, &mut fades);
        }
        // The cut copies (made only for shaders without holes) follow their originals.
        let fills = a.fill.iter().chain(a.cut.values().filter_map(|c| c[0].as_ref()));
        let [u, v] = a.shader.fill_speed;
        let uv = bevy::math::Affine2::from_translation(Vec2::new((u * t).fract(), (v * t).fract()));
        for fill in fills {
            animate_material(&mut models.materials, fill, holes, Some(a.shader.fill.alpha(t, a.persist)), Some(uv));
        }
        for edge in a.edge.iter().chain(a.cut.values().filter_map(|c| c[1].as_ref())) {
            animate_material(&mut models.materials, edge, holes, Some(a.shader.edge.alpha(t, a.persist)), None);
        }
    }
}

/// How a mesh is covered, from its own material.
#[derive(Debug, PartialEq)]
enum Cover {
    /// With the shader's overlays as they are (an opaque mesh).
    Plain,
    /// With copies cut where the mesh's own texture's alpha is below the cut (an
    /// alpha-tested mesh: a robot's body, #1717; hair drawn as a cut-out).
    CutBy(Handle<Image>, f32),
    /// Not at all: a blended mesh (beards, lashes), a refracting one (heat haze), or an
    /// alpha-tested one when the shader has holes of its own (one holes texture per
    /// material).
    None,
}

/// How a mesh drawn with `m` is covered (`shader_has_holes`: the shader eats its
/// overlays away with a holes texture).
fn cut_out_overlay(m: &NifMaterial, shader_has_holes: bool) -> Cover {
    // A refracting shape shows no surface, only the shimmer behind it: a Mister Gutsy's
    // jet haze (`##RefractCloudMesh`) was covered as a cluster of solid blobs under it
    // (#1717) [design].
    if m.extension.refract {
        return Cover::None;
    }
    match (m.base.alpha_mode, &m.base.base_color_texture) {
        (AlphaMode::Opaque, _) if m.extension.alpha_cut <= 0.0 => Cover::Plain,
        (AlphaMode::Mask(cut), Some(tex)) if m.extension.alpha_cut <= 0.0 && !shader_has_holes => Cover::CutBy(tex.clone(), cut),
        _ => Cover::None,
    }
}

/// An overlay material cut where `texture`'s alpha, at the mesh's own texture
/// coordinates, is below `cut` (0-1): the holes test turned round (nif_material.rs
/// "Holes", `holes.z`), with no edge band.
fn cut_by(mut m: NifMaterial, texture: Handle<Image>, cut: f32) -> NifMaterial {
    m.extension.holes = Vec4::new(cut * 255.0, 0.0, 1.0, 0.0);
    m.extension.holes_texture = Some(texture);
    m
}

/// Apply only differing values in one mutable access [documented: Bevy 0.20.1
/// `AssetMut::deref_mut` / `AssetMutChangeNotifier::drop` marks the asset modified].
fn animate_material(materials: &mut Assets<NifMaterial>, handle: &Handle<NifMaterial>, holes: Option<f32>, alpha: Option<f32>, uv: Option<bevy::math::Affine2>) {
    let Some(m) = materials.get(handle) else { return };
    let holes = holes.filter(|v| m.extension.holes.x != *v);
    let alpha = alpha.filter(|v| m.base.base_color.alpha() != *v);
    let uv = uv.filter(|v| m.base.uv_transform != *v);
    if holes.is_none() && alpha.is_none() && uv.is_none() {
        return;
    }
    let Some(mut m) = materials.get_mut(handle) else { return };
    if let Some(value) = holes {
        m.extension.holes.x = value;
    }
    if let Some(alpha) = alpha {
        m.base.base_color.set_alpha(alpha);
    }
    if let Some(uv) = uv {
        m.base.uv_transform = uv;
    }
}

/// The path of a texture as the records store it, under `textures\`.
fn texture_path(t: &str) -> String {
    let path = t.replace('/', "\\").to_ascii_lowercase();
    if path.starts_with("textures\\") {
        path
    } else {
        format!("textures\\{path}")
    }
}

/// Give a material the shader's holes at `value` (nif_material.rs, "Holes").
fn set_holes(m: &mut NifMaterial, h: &Holes, texture: Handle<Image>, value: f32) {
    m.extension.holes = Vec4::new(value, h.edge_width, 0.0, 0.0);
    m.extension.holes_edge = Vec4::from_array(linear(h.edge_color).to_f32_array());
    m.extension.holes_texture = Some(texture);
}

/// Up to `n` of `items`, spread evenly over them.
fn spread<T: Copy>(items: &[T], n: usize) -> Vec<T> {
    if items.len() <= n {
        return items.to_vec();
    }
    (0..n).map(|i| items[i * items.len() / n]).collect()
}

/// A 64-bit mix (splitmix64's finaliser), for the add-on models' picks.
fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Scale and fade the add-on models.
fn addon_look(models: &[(Entity, Option<Entity>)], scale: f32, alpha: f32, transforms: &mut Query<&mut Transform>, fades: &mut Query<&mut RigMaterials>) {
    for &(holder, root) in models {
        if let Ok(mut tr) = transforms.get_mut(holder) {
            if tr.scale.x != scale {
                tr.scale = Vec3::splat(scale);
            }
        }
        if let Some(mut m) = root.and_then(|r| fades.get_mut(r).ok()) {
            if m.fade != alpha {
                m.fade = alpha;
            }
        }
    }
}

/// Shrink and fade the add-on models of ended plays; take them away when done.
fn addons_leaving(mut commands: Commands, time: Res<Time>, mut st: ResMut<Shaders>, mut transforms: Query<&mut Transform>, mut fades: Query<&mut RigMaterials>) {
    let now = time.elapsed_secs();
    st.leaving.retain(|l| {
        let since_end = now - l.ended;
        if since_end >= l.addons.linger() {
            for (holder, _) in &l.models {
                commands.entity(*holder).try_despawn();
            }
            return false;
        }
        let (scale, alpha) = l.addons.look(l.ended - l.since, Some(since_end));
        addon_look(&l.models, scale, alpha, &mut transforms, &mut fades);
        true
    });
}

/// Draw actors with an alpha below 1 see-through; give the others their materials back.
#[allow(clippy::type_complexity)]
fn actor_alpha(
    mut commands: Commands,
    script: Option<Res<ScriptState>>,
    mut st: ResMut<Shaders>,
    mut materials: ResMut<Assets<NifMaterial>>,
    drawn: Query<(Entity, &RefVisual, &MeshMaterial3d<NifMaterial>, Option<&Faded>), Without<Overlay>>,
) {
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { return };
    let alpha = &rt.game.script_looks.alpha;
    let st = &mut *st;
    let mut changed: HashMap<u32, (f32, u32)> = HashMap::new();
    for (e, rv, mat, faded) in &drawn {
        let want = alpha.get(&FormId(rv.0)).copied();
        match (want, faded) {
            (None, None) => {}
            (None, Some(f)) => {
                commands.entity(e).insert(MeshMaterial3d(f.original.clone())).remove::<Faded>();
                changed.entry(rv.0).or_insert((1.0, 0)).1 += 1;
            }
            (Some(a), Some(f)) if f.alpha == a && f.made_from == f.original.id() => {}
            (Some(a), f) => {
                let original = f.map(|f| f.original.clone()).unwrap_or_else(|| mat.0.clone());
                let key = (original.id(), (a * 256.0).round() as u16);
                let copy = match st.faded.get(&key) {
                    Some(h) => h.clone(),
                    None => {
                        let Some(mut m) = materials.get(&original).cloned() else { continue };
                        // A cut-out keeps its cut while it blends.
                        if let AlphaMode::Mask(cut) = m.base.alpha_mode {
                            m.extension.alpha_cut = cut * a;
                        }
                        // A see-through actor's hair hides nothing behind it.
                        m.extension.depth_write = false;
                        if matches!(m.base.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)) {
                            m.base.alpha_mode = AlphaMode::Blend;
                        }
                        let c = m.base.base_color.alpha();
                        m.base.base_color.set_alpha(c * a);
                        let h = materials.add(m);
                        st.faded.insert(key, h.clone());
                        h
                    }
                };
                commands.entity(e).insert((MeshMaterial3d(copy), Faded { made_from: original.id(), original, alpha: a }));
                changed.entry(rv.0).or_insert((a, 0)).1 += 1;
            }
        }
    }
    for (r, (a, n)) in changed {
        st.alpha_calls += 1;
        info!("actor alpha: {} {a:.2} ({n} meshes)", rt.game.data.name(FormId(r)));
    }
}

impl Shaders {
    /// The see-through copies held, for the stress census.
    pub(crate) fn share(&self, materials: &Assets<NifMaterial>) -> nvf_assets::release::CacheShare {
        let mut s = nvf_assets::release::CacheShare::default();
        for h in self.faded.values() {
            s.material(materials, h, 1);
        }
        s
    }
}

/// Every [`SWEEP_SECS`](nvf_assets::release::SWEEP_SECS), drop the see-through copies no
/// mesh wears any more (the actor's alpha came back to 1, it unloaded, or its part was let
/// go of), so their textures go with their models (issue #904).
fn release_faded(time: Res<Time<Real>>, mut st: ResMut<Shaders>, mut since: Local<f32>) {
    *since += time.delta_secs();
    if *since >= nvf_assets::release::SWEEP_SECS {
        *since = 0.0;
        nvf_assets::release::retain_held(&mut st.faded, 1);
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<Shaders>) {
    if exit.read().next().is_none() {
        return;
    }
    info!(
        "effect shaders summary: {} played, {} ended, {} overlays drawn, {} actor alpha changes, {} meshes given holes, {} add-on models",
        st.played, st.ended, st.overlays, st.alpha_calls, st.holed, st.addons
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sky::{BODY_LAYER, SHADOW_LAYER};
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::ecs::message::Messages;
    use bevy::math::Affine2;
    use nvf_assets::nif_material::{Falloff, NO_HOLES};

    #[test]
    fn overlays_take_the_mesh_layer_or_the_default_world_layer() {
        assert_eq!(overlay_layers(None), RenderLayers::default());
        for layers in [RenderLayers::layer(BODY_LAYER), RenderLayers::layer(SHADOW_LAYER), RenderLayers::from_layers(&[0, 7]), RenderLayers::none()] {
            assert_eq!(overlay_layers(Some(&layers)), layers);
        }
        assert!(player_mesh_seen(None));
        assert!(player_mesh_seen(Some(&RenderLayers::layer(BODY_LAYER))));
        assert!(!player_mesh_seen(Some(&RenderLayers::layer(SHADOW_LAYER))));
        assert!(!player_mesh_seen(Some(&RenderLayers::none())));
    }

    #[test]
    fn player_overlays_follow_layers_in_both_directions() {
        let mut app = App::new();
        app.init_resource::<Shaders>().add_systems(Update, sync_player_effects);
        let parent = app.world_mut().spawn(RenderLayers::layer(SHADOW_LAYER)).id();
        let overlay = app.world_mut().spawn((Overlay, PlayerOverlay(parent), RenderLayers::default(), bevy::light::NotShadowCaster)).id();
        let other = app.world_mut().spawn((Overlay, RenderLayers::default())).id();
        for layer in [SHADOW_LAYER, BODY_LAYER, SHADOW_LAYER, BODY_LAYER] {
            app.world_mut().entity_mut(parent).insert(RenderLayers::layer(layer));
            app.update();
            assert_eq!(app.world().get::<RenderLayers>(overlay), Some(&RenderLayers::layer(layer)));
            assert_eq!(app.world().get::<RenderLayers>(other), Some(&RenderLayers::default()));
            assert!(app.world().get::<bevy::light::NotShadowCaster>(overlay).is_some());
        }
    }

    #[test]
    fn player_addon_emitters_are_marked_and_hidden_at_spawn() {
        let mut world = World::new();
        let holder = world.spawn(Visibility::Inherited).id();
        let emitter = world.spawn((crate::particles::AddonEmitter::new(42), ChildOf(holder))).id();
        let other = world.spawn(crate::particles::AddonEmitter::new(43)).id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        player_addon(&mut commands, holder, crate::player_body::EffectPart::Head, false);
        queue.apply(&mut world);
        assert_eq!(world.get::<Visibility>(holder), Some(&Visibility::Hidden));
        assert_eq!(world.get::<PlayerEffectEmitter>(emitter).map(|p| p.0), Some(crate::player_body::EffectPart::Head));
        assert!(world.get::<PlayerEffectEmitter>(other).is_none());
    }

    fn material_app() -> (App, Handle<NifMaterial>) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default())).init_asset::<NifMaterial>();
        let handle = app.world_mut().resource_mut::<Assets<NifMaterial>>().add(nif_material(StandardMaterial::default()));
        app.update();
        app.world_mut().resource_mut::<Messages<AssetEvent<NifMaterial>>>().clear();
        (app, handle)
    }

    fn material_events(app: &mut App) -> Vec<AssetEvent<NifMaterial>> {
        app.update();
        app.world_mut().resource_mut::<Messages<AssetEvent<NifMaterial>>>().drain().collect()
    }

    #[test]
    fn equal_material_values_emit_no_modification() {
        let (mut app, handle) = material_app();
        for _ in 0..3 {
            animate_material(&mut app.world_mut().resource_mut::<Assets<NifMaterial>>(), &handle, Some(NO_HOLES.x), Some(1.0), Some(Affine2::IDENTITY));
            assert!(material_events(&mut app).is_empty());
        }
    }

    #[test]
    fn holes_alpha_and_uv_change_in_one_modification() {
        let (mut app, handle) = material_app();
        let uv = Affine2::from_translation(Vec2::new(-0.25, 0.5));
        animate_material(&mut app.world_mut().resource_mut::<Assets<NifMaterial>>(), &handle, Some(128.0), Some(0.5), Some(uv));
        assert_eq!(material_events(&mut app), vec![AssetEvent::Modified { id: handle.id() }]);
        let materials = app.world().resource::<Assets<NifMaterial>>();
        let m = materials.get(&handle).unwrap();
        assert_eq!(m.extension.holes, Vec4::new(128.0, 0.0, 0.0, 0.0));
        assert_eq!(m.base.base_color.alpha(), 0.5);
        assert_eq!(m.base.uv_transform, uv);

        // A persistent plateau with no further scroll produces no asset event.
        animate_material(&mut app.world_mut().resource_mut::<Assets<NifMaterial>>(), &handle, Some(128.0), Some(0.5), Some(uv));
        assert!(material_events(&mut app).is_empty());
    }

    #[test]
    fn material_channels_animate_independently_without_an_epsilon() {
        let (mut app, handle) = material_app();
        let alpha = f32::from_bits(1.0f32.to_bits() - 1);
        let uv = Affine2::from_translation(Vec2::new(0.001, -0.001));
        for (holes, alpha, uv) in [(Some(255.0), None, None), (None, Some(alpha), None), (None, None, Some(uv))] {
            animate_material(&mut app.world_mut().resource_mut::<Assets<NifMaterial>>(), &handle, holes, alpha, uv);
            assert_eq!(material_events(&mut app), vec![AssetEvent::Modified { id: handle.id() }]);
        }
        let materials = app.world().resource::<Assets<NifMaterial>>();
        let m = materials.get(&handle).unwrap();
        assert_eq!(m.extension.holes.x, 255.0);
        assert_eq!(m.base.base_color.alpha(), alpha);
        assert_eq!(m.base.uv_transform, uv);
    }

    #[test]
    fn absent_material_or_values_emit_no_modification() {
        let (mut app, handle) = material_app();
        animate_material(&mut app.world_mut().resource_mut::<Assets<NifMaterial>>(), &handle, None, None, None);
        assert!(material_events(&mut app).is_empty());
        app.world_mut().resource_mut::<Assets<NifMaterial>>().remove(&handle);
        material_events(&mut app);
        animate_material(&mut app.world_mut().resource_mut::<Assets<NifMaterial>>(), &handle, Some(0.0), Some(0.0), Some(Affine2::IDENTITY));
        assert!(material_events(&mut app).is_empty());
    }

    #[test]
    fn add_on_places_spread_over_the_bones() {
        assert_eq!(spread(&[1, 2, 3], 8), vec![1, 2, 3]);
        assert_eq!(spread(&(0..16).collect::<Vec<_>>(), 4), vec![0, 4, 8, 12]);
        assert_ne!(mix(1), mix(2));
        assert_eq!(NO_HOLES.x, -1.0);
    }

    #[test]
    fn edge_is_half_strong_where_the_fall_off_says() {
        for f in [0.5f32, 0.95, 1.0, 2.5] {
            let p = Falloff { params: edge_falloff(f) };
            let half = 1.0 - 0.5f32.powf(1.0 / f);
            assert!((p.opacity(half) - 0.5).abs() < 1e-4, "fall off {f}");
            assert_eq!(p.opacity(1.0), 0.0);
        }
    }

    #[test]
    fn alpha_tested_meshes_get_overlays_cut_by_their_own_texture() {
        let tex = Handle::<Image>::default();
        let with = |mode: AlphaMode, cut: f32| {
            let mut m = nif_material(StandardMaterial { alpha_mode: mode, base_color_texture: Some(tex.clone()), ..default() });
            m.extension.alpha_cut = cut;
            m
        };
        assert_eq!(cut_out_overlay(&with(AlphaMode::Opaque, 0.0), false), Cover::Plain);
        // A Mister Gutsy's body: NiAlphaProperty test at 120 (#1717).
        assert_eq!(cut_out_overlay(&with(AlphaMode::Mask(120.0 / 255.0), 0.0), false), Cover::CutBy(tex.clone(), 120.0 / 255.0));
        // One holes texture per material: the shader's own holes win.
        assert_eq!(cut_out_overlay(&with(AlphaMode::Mask(0.5), 0.0), true), Cover::None);
        assert_eq!(cut_out_overlay(&with(AlphaMode::Blend, 0.0), false), Cover::None);
        assert_eq!(cut_out_overlay(&with(AlphaMode::Blend, 0.3), false), Cover::None);
        // A Mister Gutsy's jet haze.
        let mut haze = with(AlphaMode::Opaque, 0.0);
        haze.extension.refract = true;
        assert_eq!(cut_out_overlay(&haze, false), Cover::None);
        let m = cut_by(with(AlphaMode::Add, 0.0), tex.clone(), 0.5);
        assert_eq!(m.extension.holes, Vec4::new(127.5, 0.0, 1.0, 0.0));
        assert_eq!(m.extension.holes_texture, Some(tex));
    }
}
