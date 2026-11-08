//! SUP NVSE, batch 3 (#1008; docs/spec-notes/ext-sup3.md): the fly (TFC) camera's
//! position, angles and raw rotation, the angles from it to a point, and the positions
//! at an angle and distance from a reference or a point. The camera's place is
//! `Game::ui.camera.tfc` (`crate::functions::camera`); the engine's free camera writes it
//! and takes the scripts' changes. Probed in the original (set `sup3`).
//!
//! The `…Ex` getters and GetCalculatedPos/Alt write script variables: the VM gives them
//! here without those arguments (`fnv_script::vm` `out_params`) and writes the array's
//! values after the return value into them. The game's own compiler refused every call
//! of them in the probes [observed: sup3 cam.setposex, cam.angleex, cam.rotex,
//! calc.pos.*, calc.alt.*], so they follow the pages.

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;

fn num(args: &[XVal], k: usize) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => 0.0,
    }
}

/// An axis argument: `X`, `Y` or `Z` as 0, 1, 2.
fn axis(args: &[XVal], k: usize) -> Option<usize> {
    match args.get(k)? {
        XVal::Str(s) => match s.trim().to_ascii_uppercase().as_str() {
            "X" => Some(0),
            "Y" => Some(1),
            "Z" => Some(2),
            _ => None,
        },
        XVal::Num(x) if (0.0..3.0).contains(x) => Some(*x as usize),
        _ => None,
    }
}

/// The return value and the outputs, for the VM to write (`out_call`).
fn outputs(g: &mut Game, ret: f64, vals: &[f64]) -> XVal {
    let entries = std::iter::once(ret).chain(vals.iter().copied()).enumerate().map(|(i, v)| (XKey::Num(i as f64), XVal::Num(v))).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// Degrees to look down at a point `d` (target less camera) from level: up is negative
/// [observed: sup3 cam.calc.x, 100 across and 100 up gives -45, 100 across and 100 down
/// 45, 30 across and 40 up -53.1301].
pub fn angle_x_to(d: [f64; 3]) -> f64 {
    -d[2].atan2(d[0].hypot(d[1])).to_degrees()
}

/// The heading, in degrees, of a point `d` (target less camera, X and Y): 0 north (+Y),
/// 90 east (+X), straight south -180 [observed: sup3 cam.calc.z, 90 0 -90 -180 45 for
/// east, north, west, south and north-east].
pub fn angle_z_to(dx: f64, dy: f64) -> f64 {
    let a = dx.atan2(dy).to_degrees();
    // atan2 gives 180 for straight south; the game gives -180.
    if a >= 180.0 - 1e-9 {
        -180.0
    } else {
        a
    }
}

/// The point `dist` along heading `angle` (degrees, 0 north, 90 east) from `(x, y)`
/// [documented: GetCalculatedPos, "in front of the player" with the player's heading;
/// hypothesis: the heading as GetAngle Z gives it].
pub fn point_at(x: f64, y: f64, angle: f64, dist: f64) -> (f64, f64) {
    let (s, c) = angle.to_radians().sin_cos();
    (x + dist * s, y + dist * c)
}

pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let _ = ctx;
    let zero = XVal::Num(0.0);
    let set = |g: &mut Game, f: &dyn Fn(&mut crate::functions::camera::TfcPose)| {
        f(&mut g.ui.camera.tfc);
        g.ui.camera.tfc_set = true;
    };
    let v = match name {
        // ---- the camera's place ----
        // `axis`: its position [documented: GetTFCPos]; a camera never used is at 0 0 0,
        // and it keeps its last place when turned off [observed: sup3 cam.off.pos,
        // cam.off2.pos].
        "gettfcpos" => XVal::Num(axis(args, 0).map_or(0.0, |k| g.ui.camera.tfc.pos[k])),
        // `axis value`: returns 0 [documented: SetTFCPos; observed: sup3 cam.setpos, and
        // the next frame reads the same].
        "settfcpos" => {
            if let Some(k) = axis(args, 0) {
                let v = num(args, 1);
                set(g, &|t| t.pos[k] = v);
            }
            zero
        }
        // `x y z` [documented: SetTFCPosEx].
        "settfcposex" => {
            let p = [num(args, 0), num(args, 1), num(args, 2)];
            set(g, &|t| t.pos = p);
            zero
        }
        // Into three variables [documented: GetTFCPosEx].
        "gettfcposex" => {
            let p = g.ui.camera.tfc.pos;
            outputs(g, 0.0, &p)
        }
        // ---- angles in degrees, X (down positive) and Z (the heading) ----
        // [documented: GetTFCAngle, SetTFCAngle "X\Z"; observed: sup3 cam.on.angles, the
        // player's angles when turned on; cam.setangle.*, 30 and 45 read back, 400 and
        // -100 kept as given]. Y is neither: 0, and setting it does nothing [hypothesis].
        "gettfcangle" => XVal::Num(match axis(args, 0) {
            Some(0) => g.ui.camera.tfc.angle_x,
            Some(2) => g.ui.camera.tfc.angle_z,
            _ => 0.0,
        }),
        "settfcangle" => {
            let v = num(args, 1);
            match axis(args, 0) {
                Some(0) => set(g, &|t| t.angle_x = v),
                Some(2) => set(g, &|t| t.angle_z = v),
                _ => {}
            }
            zero
        }
        "settfcangleex" => {
            let (x, z) = (num(args, 0), num(args, 1));
            set(g, &|t| (t.angle_x, t.angle_z) = (x, z));
            zero
        }
        "gettfcangleex" => {
            let t = g.ui.camera.tfc;
            outputs(g, 0.0, &[t.angle_x, t.angle_z])
        }
        // ---- the raw rotation ----
        // X is angle X in radians; Y reads 0 and setting it changes nothing [observed:
        // sup3 cam.on.angles 9.1804° as 0.1602; cam.setrot.x, 0.5 reads angle X 28.6479;
        // cam.setrot.y, Y 1 left every value as it was]. Z: 0 [hypothesis: as Y].
        "gettfcrot" => XVal::Num(match axis(args, 0) {
            Some(0) => g.ui.camera.tfc.angle_x.to_radians(),
            _ => 0.0,
        }),
        "settfcrot" => {
            if axis(args, 0) == Some(0) {
                let v = num(args, 1).to_degrees();
                set(g, &|t| t.angle_x = v);
            }
            zero
        }
        "settfcrotex" => {
            let v = num(args, 0).to_degrees();
            set(g, &|t| t.angle_x = v);
            zero
        }
        "gettfcrotex" => {
            let x = g.ui.camera.tfc.angle_x.to_radians();
            outputs(g, 0.0, &[x, 0.0])
        }
        // ---- looking at a point from the camera ----
        // `x y z` and `x y` [documented: GetCalculatedAngleXForTFCCamera,
        // …ZForTFCCamera; observed: sup3 cam.calc.x, cam.calc.z].
        "getcalculatedanglexfortfccamera" => {
            let p = g.ui.camera.tfc.pos;
            XVal::Num(angle_x_to([num(args, 0) - p[0], num(args, 1) - p[1], num(args, 2) - p[2]]))
        }
        "getcalculatedanglezfortfccamera" => {
            let p = g.ui.camera.tfc.pos;
            XVal::Num(angle_z_to(num(args, 0) - p[0], num(args, 1) - p[1]))
        }
        // ---- points at an angle and distance ----
        // `ref.GetCalculatedPos angle dist xOut yOut` from the reference's position;
        // `GetCalculatedPosAlt angle dist xOut yOut xIn yIn` from the point [documented].
        "getcalculatedpos" => {
            let Some((p, _)) = subject.and_then(|r| g.ref_pos_rot(r)) else { return Some(Ok(zero)) };
            let (x, y) = point_at(f64::from(p[0]), f64::from(p[1]), num(args, 0), num(args, 1));
            outputs(g, 0.0, &[x, y])
        }
        "getcalculatedposalt" => {
            let (x, y) = point_at(num(args, 2), num(args, 3), num(args, 0), num(args, 1));
            outputs(g, 0.0, &[x, y])
        }
        _ => return None,
    };
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_and_points_as_the_game_gave_them() {
        let r = |x: f64| (x * 1e4).round() / 1e4;
        // [observed: sup3 cam.calc.x].
        assert_eq!([r(angle_x_to([100.0, 0.0, 100.0])), r(angle_x_to([0.0, 100.0, -100.0])), r(angle_x_to([0.0, 100.0, 0.0])), r(angle_x_to([0.0, 30.0, 40.0]))], [-45.0, 45.0, 0.0, -53.1301]);
        // [observed: sup3 cam.calc.z].
        let z = [angle_z_to(100.0, 0.0), angle_z_to(0.0, 100.0), angle_z_to(-100.0, 0.0), angle_z_to(0.0, -100.0), angle_z_to(100.0, 100.0)];
        assert_eq!(z.map(r), [90.0, 0.0, -90.0, -180.0, 45.0]);
        let (x, y) = point_at(100.0, 200.0, 90.0, 25.0);
        assert_eq!((r(x), r(y)), (125.0, 200.0));
    }
}
