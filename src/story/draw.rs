//! Drawing what a scene is made of: people, animals, vehicles, boats and the
//! props they handle, and the water, debris and snow that physics leaves.
//!
//! Everything is generic over `Surface`, so the still picture and the live
//! layer draw the same figure; and everything is in design units through a
//! `Layout`, so a view needs only say where, how big (perspective) and in
//! what light. A `Tone` carries that light: the colour distant things sink
//! to, and how much of their own colour shows (colour comes with daylight).

use std::f32::consts::PI;

use super::actors::{Actor, ActorPose, Kind, Role};
use super::physics::{Body, BodyKind, Patch, PatchKind};
use crate::color::{mix, shade, Rgb};
use crate::gfx::{Draw, Surface, P};
use crate::props::Layout;

/// The light something stands in.
#[derive(Clone, Copy, Debug)]
pub struct Tone {
    /// the colour things in shade sink to at this depth
    pub near: Rgb,
    /// how much of a thing's own colour shows: 0 at night, 1 in full day
    pub tint: f32,
    /// the sky behind, for glass
    pub sky: Rgb,
}

impl Tone {
    pub fn new(near: Rgb, tint: f32, sky: Rgb) -> Tone {
        Tone { near, tint: tint.clamp(0.0, 1.0), sky }
    }

    pub fn paint(&self, own: Rgb) -> Rgb {
        // props and people are what the eye is meant to find: they keep their own colour even
        // after dark, a little dimmed
        mix(self.near, own, 0.34 + 0.52 * self.tint)
    }
}

/// Everything a scene has in it that is not part of the view's own drawing: puddles, loose debris,
/// and the people, animals and vehicles, far to near, each behind what hides it where it stands.
pub fn scene(d: &mut Draw<crate::gfx::Frame>, lay: &Layout, sc: &crate::scene::Scene, stage: &dyn crate::views::Stage, world: &super::StoryWorld, t: f32) {
    let tone = stage.story_tone(sc, None);
    d.s.behind = None;
    for p in &world.physics().patches {
        puddle(d, lay, p, &tone, 1.0);
    }
    for b in world.bodies() {
        if !matches!(b.kind, BodyKind::Thrown(_)) {
            body(d, lay, b, &tone, 1.0);
        }
    }
    let mut order: Vec<&Actor> = world.actors().iter().collect();
    order.sort_by(|a, b| a.pos.1.partial_cmp(&b.pos.1).unwrap_or(std::cmp::Ordering::Equal));
    for a in order {
        d.s.behind = a.layer.map(|l| l as usize);
        if stage.story_actor(d, sc, a, t) {
            continue;
        }
        let tone = stage.story_tone(sc, a.layer);
        match a.kind {
            Kind::Person => person(d, lay, a, &tone, t, 1.0),
            Kind::Cat => cat(d, lay, a, &tone, t, 1.0),
            Kind::Bird => bird(d, lay, a, &tone, t, 1.0),
            Kind::Boat => boat(d, lay, a, &tone, t, 1.0),
            Kind::Car | Kind::Van | Kind::Truck | Kind::Train => vehicle(d, lay, a, &tone, t, 1.0),
        }
    }
    d.s.behind = None;
}

pub fn ink<S: Surface>(d: &mut Draw<S>, lay: &Layout, pts: &[P], w: f32, col: Rgb, a: f32) {
    let px: Vec<P> = pts.iter().map(|p| (lay.dx(p.0), lay.dy(p.1))).collect();
    d.path(&px, lay.d(w).max(0.7), col, a);
}

pub fn oval<S: Surface>(d: &mut Draw<S>, lay: &Layout, x: f32, y: f32, rx: f32, ry: f32, col: Rgb, a: f32) {
    d.ellipse(lay.dx(x), lay.dy(y), lay.d(rx).max(0.4), lay.d(ry).max(0.4), col, a);
}

pub fn bar<S: Surface>(d: &mut Draw<S>, lay: &Layout, r: [f32; 4], col: Rgb, a: f32) {
    d.rect(lay.dx(r[0]), lay.dy(r[1]), lay.dx(r[2]), lay.dy(r[3]), col, a);
}

pub fn poly<S: Surface>(d: &mut Draw<S>, lay: &Layout, pts: &[P], col: Rgb, a: f32) {
    let px: Vec<P> = pts.iter().map(|p| (lay.dx(p.0), lay.dy(p.1))).collect();
    d.poly(&px, col, a);
}

const SKIN: [Rgb; 4] = [[226.0, 190.0, 160.0], [196.0, 150.0, 118.0], [150.0, 106.0, 80.0], [238.0, 208.0, 182.0]];
const COATS: [Rgb; 6] = [[186.0, 92.0, 84.0], [212.0, 168.0, 92.0], [120.0, 150.0, 132.0], [104.0, 132.0, 170.0], [214.0, 204.0, 184.0], [130.0, 96.0, 128.0]];
const HIVIS: Rgb = [236.0, 150.0, 48.0];
const HARDHAT: Rgb = [238.0, 206.0, 70.0];
const POLICE: Rgb = [48.0, 66.0, 112.0];
const OVERALLS: Rgb = [70.0, 104.0, 150.0];
const OILSKIN: Rgb = [226.0, 186.0, 58.0];

/// What the role wears: the top, the legs, the hat (colour and height), and a hi-vis band.
fn kit(role: Role, look: u32) -> (Rgb, Rgb, Option<(Rgb, f32)>, bool) {
    let coat = COATS[look as usize % COATS.len()];
    match role {
        Role::Worker => (HIVIS, [56.0, 62.0, 78.0], Some((HARDHAT, 1.0)), true),
        Role::Roofer => ([150.0, 112.0, 82.0], [64.0, 58.0, 54.0], Some((HARDHAT, 1.0)), false),
        Role::Plumber => (OVERALLS, OVERALLS, Some(([80.0, 90.0, 104.0], 0.7)), false),
        Role::Electrician | Role::Technician => (HIVIS, [52.0, 58.0, 70.0], Some((HARDHAT, 1.0)), true),
        Role::Police => (POLICE, [38.0, 46.0, 72.0], Some(([34.0, 44.0, 78.0], 1.1)), false),
        Role::Courier => ([196.0, 74.0, 66.0], [48.0, 54.0, 66.0], Some(([36.0, 40.0, 50.0], 0.7)), false),
        Role::Keeper => (OILSKIN, [50.0, 60.0, 78.0], Some((OILSKIN, 1.0)), false),
        Role::Dockhand => ([84.0, 112.0, 138.0], [58.0, 64.0, 76.0], Some(([60.0, 70.0, 90.0], 0.7)), false),
        Role::Staff => ([60.0, 108.0, 112.0], [44.0, 56.0, 66.0], Some(([40.0, 82.0, 88.0], 0.8)), true),
        Role::Passenger | Role::Resident | Role::Customer | Role::Owner => {
            (coat, [56.0, 60.0, 74.0], if look % 3 == 0 { Some((COATS[(look as usize + 3) % COATS.len()], 0.8)) } else { None }, false)
        }
    }
}

/// A person, feet at the actor's position. `t` is a clock in seconds for their steps and strokes.
pub fn person<S: Surface>(d: &mut Draw<S>, lay: &Layout, a: &Actor, tone: &Tone, t: f32, alpha: f32) {
    let (x, y, s, f) = (a.pos.0, a.pos.1, a.scale, a.facing);
    let (top, legs, hat, hivis) = kit(a.role, a.look);
    let (top, legs) = (tone.paint(top), tone.paint(legs));
    let skin = tone.paint(SKIN[a.look as usize % SKIN.len()]);
    let phase = t * match a.pose {
        ActorPose::Run => 12.0,
        ActorPose::Climb => 4.0,
        _ => 7.0,
    } + a.id.slot as f32 * 1.7;
    let swing = phase.sin();
    let moving = matches!(a.pose, ActorPose::Walk | ActorPose::Run | ActorPose::Carry | ActorPose::Climb);
    let (stride, lift) = match a.pose {
        ActorPose::Run => (10.0, 3.2),
        ActorPose::Climb => (4.0, 0.0),
        _ if moving => (6.0, 0.0),
        _ => (0.0, 0.0),
    };
    let crouch = if matches!(a.pose, ActorPose::Work | ActorPose::Sit) { 0.28 } else { 0.0 };
    let hip = (x, y - (19.0 - 19.0 * crouch) * s - lift * s * swing.abs());
    let shoulder = (x + f * if a.pose == ActorPose::Run { 2.5 * s } else { 0.0 }, hip.1 - 15.0 * s);
    let head = (shoulder.0 + f * 0.8 * s, shoulder.1 - 6.0 * s);
    // legs
    if a.pose == ActorPose::Climb {
        let up = (phase.sin() * 4.0) * s;
        ink(d, lay, &[hip, (x - 2.0 * s, y + up)], 3.4 * s, legs, alpha);
        ink(d, lay, &[hip, (x + 2.0 * s, y - up)], 3.4 * s, legs, alpha);
    } else if crouch > 0.0 {
        let knee = (x + f * 9.0 * s, hip.1 + 3.0 * s);
        ink(d, lay, &[hip, knee, (x + f * 3.0 * s, y)], 3.4 * s, legs, alpha);
        ink(d, lay, &[hip, (knee.0 - f * 2.0 * s, knee.1), (x - f * 3.0 * s, y)], 3.4 * s, shade(legs, -0.1), alpha);
    } else {
        // standing, the feet are a little apart; walking, they swing
        let apart = if moving { 0.0 } else { 2.2 };
        ink(d, lay, &[hip, (x + (swing * stride + apart) * s, y - lift * s * (-swing).max(0.0))], 3.4 * s, legs, alpha);
        ink(d, lay, &[hip, (x - (swing * stride + apart) * s, y - lift * s * swing.max(0.0))], 3.4 * s, shade(legs, -0.1), alpha);
    }
    // torso
    ink(d, lay, &[hip, shoulder], 10.0 * s, top, alpha);
    if hivis {
        ink(d, lay, &[(hip.0, hip.1 - 6.0 * s), (shoulder.0, shoulder.1 + 4.0 * s)], 10.2 * s, tone.paint([226.0, 232.0, 214.0]), 0.55 * alpha);
    }
    // arms: by what they are doing
    let arm = shade(top, -0.12);
    let sh = (shoulder.0, shoulder.1 + 1.5 * s);
    let reach = |dx: f32, dy: f32| (sh.0 + f * dx * s, sh.1 + dy * s);
    match a.pose {
        ActorPose::Walk | ActorPose::Run => {
            let r = if a.pose == ActorPose::Run { 9.0 } else { 5.5 };
            ink(d, lay, &[sh, (sh.0 - swing * r * s * f, sh.1 + 11.0 * s)], 2.8 * s, arm, alpha);
            ink(d, lay, &[sh, (sh.0 + swing * r * s * f, sh.1 + 11.0 * s)], 2.8 * s, arm, alpha);
        }
        ActorPose::Carry => {
            let hand = reach(9.0, 7.0);
            ink(d, lay, &[sh, reach(4.0, 8.0), hand], 2.8 * s, arm, alpha);
        }
        ActorPose::Operate => {
            ink(d, lay, &[sh, reach(8.0, 3.0), reach(13.0, 4.0 + (t * 3.0).sin() * 1.0)], 2.8 * s, arm, alpha);
            ink(d, lay, &[sh, reach(3.0, 9.0)], 2.8 * s, arm, alpha);
        }
        ActorPose::Work => {
            // a rhythmic stroke
            let stroke = (t * 6.0 + a.id.slot as f32).sin();
            let hand = reach(9.0 + stroke * 3.0, -3.0 + stroke * 6.0);
            ink(d, lay, &[sh, reach(5.0, 2.0), hand], 2.8 * s, arm, alpha);
            ink(d, lay, &[sh, reach(4.0, 8.0)], 2.8 * s, arm, alpha);
            ink(d, lay, &[hand, (hand.0 + f * 5.0 * s, hand.1 + 7.0 * s)], 1.5 * s, tone.paint([120.0, 110.0, 100.0]), alpha);
        }
        ActorPose::Climb => {
            let up = phase.cos() * 5.0;
            ink(d, lay, &[sh, (sh.0 + f * 3.0 * s, sh.1 - 11.0 * s - up * s)], 2.8 * s, arm, alpha);
            ink(d, lay, &[sh, (sh.0 + f * 3.0 * s, sh.1 - 11.0 * s + up * s)], 2.8 * s, arm, alpha);
        }
        ActorPose::React => {
            ink(d, lay, &[sh, reach(7.0, -9.0)], 2.8 * s, arm, alpha);
            ink(d, lay, &[sh, reach(-6.0, -8.0)], 2.8 * s, arm, alpha);
        }
        ActorPose::Wave => {
            let w = (t * 8.0).sin() * 4.0;
            ink(d, lay, &[sh, reach(6.0, -6.0), reach(9.0 + w, -13.0)], 2.8 * s, arm, alpha);
            ink(d, lay, &[sh, reach(1.0, 11.0)], 2.8 * s, arm, alpha);
        }
        ActorPose::Look => {
            ink(d, lay, &[sh, reach(7.0, -2.0), reach(9.0, -9.0)], 2.8 * s, arm, alpha);
            ink(d, lay, &[sh, reach(1.0, 11.0)], 2.8 * s, arm, alpha);
        }
        ActorPose::Idle | ActorPose::Sit => {
            ink(d, lay, &[sh, reach(1.5, 11.0)], 2.8 * s, arm, alpha);
            ink(d, lay, &[sh, reach(-2.0, 11.0)], 2.8 * s, shade(arm, -0.1), alpha);
        }
    }
    // head and hat
    oval(d, lay, head.0, head.1, 4.2 * s, 4.5 * s, skin, alpha);
    if let Some((col, tall)) = hat {
        let c = tone.paint(col);
        oval(d, lay, head.0 - f * 0.4 * s, head.1 - 2.2 * s, 4.7 * s, 3.0 * s * tall, c, alpha);
        if a.role == Role::Police || a.role == Role::Courier {
            ink(d, lay, &[(head.0 + f * 1.5 * s, head.1 - 1.6 * s), (head.0 + f * 6.0 * s, head.1 - 1.2 * s)], 1.4 * s, c, alpha);
        }
    }
    // a person carrying something shows it in their hand
    if a.pose == ActorPose::Carry {
        let hand = reach(9.0, 7.0);
        let _ = hand;
    }
}

/// A cat or other small animal, feet at the actor's position.
pub fn cat<S: Surface>(d: &mut Draw<S>, lay: &Layout, a: &Actor, tone: &Tone, t: f32, alpha: f32) {
    let (x, y, s, f) = (a.pos.0, a.pos.1, a.scale, a.facing);
    let fur = tone.paint([[188.0, 142.0, 98.0], [92.0, 90.0, 96.0], [226.0, 220.0, 206.0]][a.look as usize % 3]);
    let moving = matches!(a.pose, ActorPose::Walk | ActorPose::Run);
    let trot = if moving { (t * 10.0 + a.id.slot as f32).sin() * 2.2 } else { 0.0 };
    let sit = !moving && matches!(a.pose, ActorPose::Sit | ActorPose::Idle | ActorPose::Look);
    if sit {
        oval(d, lay, x, y - 6.0 * s, 5.5 * s, 7.0 * s, fur, alpha);
        oval(d, lay, x + f * 3.5 * s, y - 14.5 * s, 3.2 * s, 3.0 * s, fur, alpha);
        for e in [-1.0, 1.0] {
            poly(d, lay, &[(x + f * 3.5 * s + e * 2.6 * s, y - 15.0 * s), (x + f * 3.5 * s + e * 3.2 * s, y - 19.5 * s), (x + f * 3.5 * s + e * 0.6 * s, y - 16.8 * s)], fur, alpha);
        }
        ink(d, lay, &[(x - f * 4.5 * s, y - 1.0 * s), (x - f * 9.0 * s, y - 3.0 * s + (t * 2.0).sin() * 2.0 * s)], 1.5 * s, fur, alpha);
    } else {
        for (o, sgn) in [(-6.0, 1.0), (6.0, -1.0)] {
            ink(d, lay, &[(x + o * s, y - 6.0 * s), (x + (o + sgn * trot) * s, y)], 1.8 * s, fur, alpha);
        }
        ink(d, lay, &[(x - 7.0 * s, y - 8.0 * s), (x + 7.0 * s, y - 8.0 * s)], 7.0 * s, fur, alpha);
        oval(d, lay, x + f * 9.5 * s, y - 11.0 * s, 3.6 * s, 3.2 * s, fur, alpha);
        for e in [-1.0, 1.0] {
            poly(d, lay, &[(x + f * 9.5 * s + e * 2.2 * s, y - 13.0 * s), (x + f * 9.5 * s + e * 2.8 * s, y - 16.5 * s), (x + f * 9.5 * s + e * 0.4 * s, y - 14.0 * s)], fur, alpha);
        }
        ink(d, lay, &[(x - f * 7.5 * s, y - 9.0 * s), (x - f * 12.0 * s, y - 14.0 * s + trot * 0.4 * s)], 1.5 * s, fur, alpha);
    }
}

/// A small bird in flight or on a perch, as the live layer draws them.
pub fn bird<S: Surface>(d: &mut Draw<S>, lay: &Layout, a: &Actor, tone: &Tone, t: f32, alpha: f32) {
    let flying = matches!(a.state, super::actors::ActorState::Moving);
    let beat = if flying { (t * 14.0 + a.id.slot as f32 * 2.1).sin() } else { 0.0 };
    crate::props::bird(d, lay.dx(a.pos.0), lay.dy(a.pos.1), lay.d(14.0 * a.scale), 0.2 + 0.75 * beat.abs(), tone.paint([40.0, 44.0, 52.0]), alpha);
}

/// A road vehicle at the actor's position (centre of its wheelbase on the road).
pub fn vehicle<S: Surface>(d: &mut Draw<S>, lay: &Layout, a: &Actor, tone: &Tone, t: f32, alpha: f32) {
    let (x, y, s, f) = (a.pos.0, a.pos.1, a.scale, a.facing);
    let (w, h) = match a.kind {
        Kind::Truck => (150.0, 54.0),
        Kind::Van => (124.0, 48.0),
        _ => (110.0, 36.0),
    };
    let paint = [[226.0, 214.0, 190.0], [68.0, 96.0, 142.0], [196.0, 66.0, 58.0], [236.0, 170.0, 52.0]][a.look as usize % 4];
    let paint = if a.role == Role::Police { [230.0, 232.0, 238.0] } else { paint };
    let body = tone.paint(paint);
    let dark = shade(body, -0.25);
    let (w, h) = (w * s, h * s);
    let glass = mix(tone.sky, tone.near, 0.45);
    // the lower body and the cab or roof
    poly(d, lay, &[(x - w / 2.0, y - 8.0 * s), (x + w / 2.0, y - 8.0 * s), (x + w / 2.0, y - h * 0.58), (x - w / 2.0, y - h * 0.58)], body, alpha);
    let cab = if matches!(a.kind, Kind::Car) { (0.62, 0.20) } else { (0.30, 0.0) };
    let (cx0, cx1) = (x + f * w * (0.5 - cab.0), x + f * w * (0.5 - cab.1));
    let (lo, hi) = (cx0.min(cx1), cx0.max(cx1));
    poly(d, lay, &[(lo, y - h * 0.58), (hi, y - h * 0.58), (hi - f * 6.0 * s, y - h), (lo + f * 3.0 * s, y - h)], body, alpha);
    poly(d, lay, &[(lo + 2.0 * s, y - h * 0.62), (hi - 2.0 * s, y - h * 0.62), (hi - f * 7.0 * s, y - h * 0.94), (lo + f * 3.0 * s + 2.0 * s, y - h * 0.94)], glass, 0.85 * alpha);
    ink(d, lay, &[(x - w / 2.0, y - 8.0 * s), (x + w / 2.0, y - 8.0 * s)], 2.0 * s, dark, alpha);
    // a utility bed: a ladder rack and a light on top
    if matches!(a.kind, Kind::Van | Kind::Truck) && a.role != Role::Courier {
        ink(d, lay, &[(x - f * w * 0.42, y - h - 3.0 * s), (x + f * w * 0.05, y - h - 3.0 * s)], 1.6 * s, dark, alpha);
        ink(d, lay, &[(x - f * w * 0.40, y - h - 1.0 * s), (x - f * w * 0.40, y - h - 5.0 * s)], 1.2 * s, dark, alpha);
        let blink = ((t * 3.0).sin() > 0.3) as u8 as f32;
        oval(d, lay, x + f * w * 0.22, y - h - 2.0 * s, 3.0 * s, 1.8 * s, mix(tone.paint([240.0, 160.0, 40.0]), [255.0, 200.0, 90.0], blink), alpha);
    }
    if a.role == Role::Police {
        let on = ((t * 4.0).sin() > 0.0) as u8 as f32;
        oval(d, lay, x - 4.0 * s, y - h - 2.0 * s, 3.0 * s, 1.8 * s, mix([60.0, 80.0, 180.0], [120.0, 160.0, 255.0], on), alpha);
        oval(d, lay, x + 4.0 * s, y - h - 2.0 * s, 3.0 * s, 1.8 * s, mix([180.0, 60.0, 60.0], [255.0, 120.0, 110.0], 1.0 - on), alpha);
    }
    // wheels
    for wx in [-0.30, 0.32] {
        oval(d, lay, x + wx * w, y - 5.0 * s, 8.0 * s, 8.0 * s, tone.paint([30.0, 32.0, 38.0]), alpha);
        oval(d, lay, x + wx * w, y - 5.0 * s, 3.4 * s, 3.4 * s, tone.paint([150.0, 154.0, 160.0]), alpha);
    }
}

/// A small boat at the actor's position (waterline).
pub fn boat<S: Surface>(d: &mut Draw<S>, lay: &Layout, a: &Actor, tone: &Tone, t: f32, alpha: f32) {
    let (x, y, s, f) = (a.pos.0, a.pos.1, a.scale, a.facing);
    let bob = (t * 1.3 + a.id.slot as f32).sin() * 1.2 * s;
    let hull = tone.paint([[154.0, 74.0, 60.0], [60.0, 92.0, 120.0], [210.0, 200.0, 178.0]][a.look as usize % 3]);
    let y = y + bob;
    poly(d, lay, &[(x - 30.0 * s, y - 9.0 * s), (x + 30.0 * s, y - 9.0 * s), (x + f * 36.0 * s, y - 11.0 * s), (x + 22.0 * s, y + 2.0 * s), (x - 22.0 * s, y + 2.0 * s)], hull, alpha);
    ink(d, lay, &[(x - 30.0 * s, y - 9.0 * s), (x + 30.0 * s, y - 9.0 * s)], 1.6 * s, shade(hull, 0.2), alpha);
    // a little cabin and a mast with a lamp
    poly(d, lay, &[(x - f * 4.0 * s, y - 9.0 * s), (x - f * 4.0 * s, y - 20.0 * s), (x + f * 12.0 * s, y - 20.0 * s), (x + f * 14.0 * s, y - 9.0 * s)], shade(hull, 0.12), alpha);
    ink(d, lay, &[(x - f * 16.0 * s, y - 9.0 * s), (x - f * 16.0 * s, y - 32.0 * s)], 1.3 * s, tone.paint([70.0, 62.0, 56.0]), alpha);
}

// ── props ────────────────────────────────────────────────────────────────────

pub fn ladder<S: Surface>(d: &mut Draw<S>, lay: &Layout, foot: P, top: P, scale: f32, tone: &Tone, alpha: f32) {
    let wood = tone.paint([176.0, 138.0, 90.0]);
    let (dx, dy) = (top.0 - foot.0, top.1 - foot.1);
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let (nx, ny) = (-dy / len * 3.0 * scale, dx / len * 3.0 * scale);
    for k in [-1.0, 1.0] {
        ink(d, lay, &[(foot.0 + nx * k, foot.1 + ny * k), (top.0 + nx * k, top.1 + ny * k)], 1.5 * scale, wood, alpha);
    }
    let rungs = ((len / (7.0 * scale)) as usize).clamp(2, 24);
    for r in 1..rungs {
        let f = r as f32 / rungs as f32;
        let (cx, cy) = (foot.0 + dx * f, foot.1 + dy * f);
        ink(d, lay, &[(cx - nx, cy - ny), (cx + nx, cy + ny)], 1.1 * scale, wood, alpha);
    }
}

pub fn bucket<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, scale: f32, tone: &Tone, fill: f32, alpha: f32) {
    let tin = tone.paint([150.0, 156.0, 164.0]);
    poly(d, lay, &[(at.0 - 5.0 * scale, at.1 - 9.0 * scale), (at.0 + 5.0 * scale, at.1 - 9.0 * scale), (at.0 + 3.6 * scale, at.1), (at.0 - 3.6 * scale, at.1)], tin, alpha);
    if fill > 0.0 {
        let top = at.1 - 9.0 * scale + 6.0 * scale * (1.0 - fill.clamp(0.0, 1.0));
        poly(d, lay, &[(at.0 - 4.6 * scale, top), (at.0 + 4.6 * scale, top), (at.0 + 5.0 * scale, at.1 - 9.0 * scale), (at.0 - 5.0 * scale, at.1 - 9.0 * scale)], tone.paint([112.0, 168.0, 210.0]), 0.9 * alpha);
    }
    ink(d, lay, &[(at.0 - 5.0 * scale, at.1 - 9.0 * scale), (at.0, at.1 - 15.0 * scale), (at.0 + 5.0 * scale, at.1 - 9.0 * scale)], 0.9 * scale, shade(tin, -0.2), alpha);
}

pub fn cone<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, scale: f32, tone: &Tone, alpha: f32) {
    poly(d, lay, &[(at.0 - 3.0 * scale, at.1), (at.0 + 3.0 * scale, at.1), (at.0, at.1 - 12.0 * scale)], tone.paint([236.0, 112.0, 48.0]), alpha);
    ink(d, lay, &[(at.0 - 1.6 * scale, at.1 - 6.0 * scale), (at.0 + 1.6 * scale, at.1 - 6.0 * scale)], 1.4 * scale, tone.paint([240.0, 240.0, 232.0]), alpha);
    ink(d, lay, &[(at.0 - 4.2 * scale, at.1), (at.0 + 4.2 * scale, at.1)], 1.4 * scale, tone.paint([226.0, 100.0, 44.0]), alpha);
}

/// A striped barrier across `width` design units.
pub fn barrier<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, width: f32, scale: f32, tone: &Tone, alpha: f32) {
    let (x0, x1, y) = (at.0 - width / 2.0, at.0 + width / 2.0, at.1 - 10.0 * scale);
    for x in [x0 + 3.0 * scale, x1 - 3.0 * scale] {
        ink(d, lay, &[(x, at.1), (x, y)], 1.4 * scale, tone.paint([90.0, 90.0, 96.0]), alpha);
    }
    let n = ((width / (6.0 * scale)) as usize).clamp(2, 16);
    for k in 0..n {
        let a = x0 + width * k as f32 / n as f32;
        let b = x0 + width * (k + 1) as f32 / n as f32;
        let col = if k % 2 == 0 { [236.0, 236.0, 230.0] } else { [220.0, 60.0, 48.0] };
        bar(d, lay, [a, y - 2.4 * scale, b, y + 2.4 * scale], tone.paint(col), alpha);
    }
}

pub fn crate_box<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, scale: f32, turn: f32, tone: &Tone, alpha: f32) {
    let wood = tone.paint([176.0, 136.0, 86.0]);
    let (c, s) = (turn.cos(), turn.sin());
    let corner = |x: f32, y: f32| (at.0 + (x * c - y * s) * scale, at.1 + (x * s + y * c) * scale);
    poly(d, lay, &[corner(-7.0, -7.0), corner(7.0, -7.0), corner(7.0, 7.0), corner(-7.0, 7.0)], wood, alpha);
    ink(d, lay, &[corner(-7.0, -7.0), corner(7.0, 7.0)], 1.0 * scale, shade(wood, -0.25), alpha);
    ink(d, lay, &[corner(-7.0, 7.0), corner(7.0, -7.0)], 1.0 * scale, shade(wood, -0.25), alpha);
}

pub fn parcel<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, scale: f32, turn: f32, look: u32, tone: &Tone, alpha: f32) {
    let paper = tone.paint([[214.0, 178.0, 120.0], [226.0, 220.0, 204.0], [190.0, 130.0, 100.0]][look as usize % 3]);
    let (c, s) = (turn.cos(), turn.sin());
    let corner = |x: f32, y: f32| (at.0 + (x * c - y * s) * scale, at.1 + (x * s + y * c) * scale);
    poly(d, lay, &[corner(-6.0, -4.5), corner(6.0, -4.5), corner(6.0, 4.5), corner(-6.0, 4.5)], paper, alpha);
    ink(d, lay, &[corner(0.0, -4.5), corner(0.0, 4.5)], 1.0 * scale, shade(paper, -0.3), alpha);
}

#[allow(dead_code)] // the shared kit: no view's definition has called for it yet
pub fn toolbag<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, scale: f32, tone: &Tone, alpha: f32) {
    let c = tone.paint([120.0, 84.0, 62.0]);
    poly(d, lay, &[(at.0 - 7.0 * scale, at.1), (at.0 + 7.0 * scale, at.1), (at.0 + 6.0 * scale, at.1 - 7.0 * scale), (at.0 - 6.0 * scale, at.1 - 7.0 * scale)], c, alpha);
    ink(d, lay, &[(at.0 - 4.0 * scale, at.1 - 7.0 * scale), (at.0, at.1 - 11.0 * scale), (at.0 + 4.0 * scale, at.1 - 7.0 * scale)], 1.0 * scale, shade(c, -0.3), alpha);
}

// ── what physics leaves ──────────────────────────────────────────────────────

pub fn body<S: Surface>(d: &mut Draw<S>, lay: &Layout, b: &Body, tone: &Tone, alpha: f32) {
    let (x, y) = (b.pos.0, b.pos.1);
    match b.kind {
        BodyKind::Droplet => oval(d, lay, x, y, 1.1, 1.8, tone.paint([150.0, 196.0, 232.0]), 0.8 * alpha),
        BodyKind::Flake => oval(d, lay, x, y, 1.2, 1.2, [244.0, 246.0, 250.0], 0.85 * alpha),
        BodyKind::Snowball => oval(d, lay, x, y, b.radius, b.radius, [238.0, 242.0, 248.0], alpha),
        BodyKind::Fragment => {
            let (c, s) = (b.turn.cos(), b.turn.sin());
            let r = b.radius;
            let col = match b.material {
                super::model::Material::Glass => mix(tone.sky, [214.0, 232.0, 240.0], 0.5),
                super::model::Material::Wood => tone.paint([150.0, 108.0, 70.0]),
                super::model::Material::Stone => tone.paint([150.0, 142.0, 134.0]),
                _ => tone.paint([130.0, 134.0, 142.0]),
            };
            let r = r * 1.7;
            poly(d, lay, &[(x + c * r * 1.6, y + s * r * 1.6), (x - s * r, y + c * r), (x - c * r * 1.2, y - s * r * 1.2)], col, alpha);
        }
        BodyKind::Thrown(_) => {}
    }
}

/// A puddle: flat, wet, catching the sky.
pub fn puddle<S: Surface>(d: &mut Draw<S>, lay: &Layout, p: &Patch, tone: &Tone, alpha: f32) {
    let r = (3.0 + 5.0 * p.amount.min(6.0)).min(34.0);
    match p.kind {
        PatchKind::Water => {
            oval(d, lay, p.at.0, p.at.1 + 1.0, r, r * 0.22, mix(tone.sky, tone.near, 0.4), 0.7 * alpha);
            oval(d, lay, p.at.0 - r * 0.2, p.at.1 + 0.6, r * 0.5, r * 0.08, [236.0, 244.0, 250.0], 0.35 * alpha * tone.tint.max(0.25));
        }
        PatchKind::Snow => {
            oval(d, lay, p.at.0, p.at.1 - 1.0, r * 0.8, r * 0.34 + p.amount * 0.6, [240.0, 244.0, 250.0], 0.95 * alpha);
        }
    }
}

/// A rope, between two points, sagging.
#[allow(dead_code)] // the shared kit: no view's definition has called for it yet
pub fn rope<S: Surface>(d: &mut Draw<S>, lay: &Layout, a: P, b: P, slack: f32, scale: f32, tone: &Tone, alpha: f32) {
    let pts: Vec<P> = (0..=10)
        .map(|k| {
            let f = k as f32 / 10.0;
            (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f + (f * PI).sin() * slack)
        })
        .collect();
    ink(d, lay, &pts, 1.4 * scale, tone.paint([196.0, 172.0, 120.0]), alpha);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::{Canvas, Shaper};
    use crate::story::actors::{ActorSpec, ActorSystem, RouteGraph};
    use crate::story::model::{ActorId, RouteId};

    fn actor(kind: Kind, role: Role, pose: ActorPose, at: P, scale: f32, look: u32) -> Actor {
        let mut g = RouteGraph::default();
        g.add_node(1, at, super::super::actors::Mode::Walk, scale);
        let mut sys = ActorSystem::new();
        let id = sys.spawn(ActorSpec { kind, role, at: RouteId(1), look }, &g).unwrap();
        let mut a = sys.get(id).unwrap().clone();
        a.pose = pose;
        a.state = if pose == ActorPose::Walk { super::super::actors::ActorState::Moving } else { super::super::actors::ActorState::Idle };
        let _: ActorId = a.id;
        a
    }

    #[test]
    fn every_pose_draws_something_inside_its_own_box() {
        let lay = Layout::new(640, 400, 0.7);
        let tone = Tone::new([60.0, 64.0, 74.0], 1.0, [150.0, 180.0, 220.0]);
        let poses = [
            ActorPose::Idle,
            ActorPose::Look,
            ActorPose::Walk,
            ActorPose::Run,
            ActorPose::Carry,
            ActorPose::Operate,
            ActorPose::Work,
            ActorPose::Climb,
            ActorPose::React,
            ActorPose::Wave,
            ActorPose::Sit,
        ];
        let roles = [Role::Worker, Role::Roofer, Role::Plumber, Role::Police, Role::Courier, Role::Keeper, Role::Resident];
        for (n, pose) in poses.iter().enumerate() {
            let role = roles[n % roles.len()];
            let a = actor(Kind::Person, role, *pose, (800.0, 700.0), 1.0, n as u32);
            let mut cv = Canvas::new(640, 400);
            cv.rows(&vec![[90.0, 100.0, 120.0]; 400]);
            let before = cv.px.clone();
            let mut sh = Shaper::new();
            let mut d = Draw::new(&mut cv, &mut sh);
            person(&mut d, &lay, &a, &tone, 0.37, 1.0);
            // changed pixels sit around the feet, inside a figure's height
            let (cx, cy) = (lay.dx(800.0), lay.dy(700.0));
            let mut touched = 0;
            for y in 0..400usize {
                for x in 0..640usize {
                    if cv.px[(y * 640 + x) * 3..(y * 640 + x) * 3 + 3] != before[(y * 640 + x) * 3..(y * 640 + x) * 3 + 3] {
                        touched += 1;
                        assert!((x as f32 - cx).abs() < lay.d(40.0) && (y as f32) < cy + lay.d(8.0) && (y as f32) > cy - lay.d(70.0), "{:?} paints outside its box at {},{}", pose, x, y);
                    }
                }
            }
            assert!(touched > 30, "{:?} drew {} pixels", pose, touched);
        }
    }

    #[test]
    fn vehicles_boats_and_animals_draw() {
        let lay = Layout::new(640, 400, 0.7);
        let tone = Tone::new([60.0, 64.0, 74.0], 0.8, [150.0, 180.0, 220.0]);
        for (kind, role) in [(Kind::Van, Role::Worker), (Kind::Truck, Role::Worker), (Kind::Car, Role::Police), (Kind::Boat, Role::Dockhand), (Kind::Cat, Role::Resident), (Kind::Bird, Role::Resident)] {
            let a = actor(kind, role, ActorPose::Idle, (500.0, 600.0), 1.0, 1);
            let mut cv = Canvas::new(640, 400);
            let mut sh = Shaper::new();
            let mut d = Draw::new(&mut cv, &mut sh);
            match kind {
                Kind::Van | Kind::Truck | Kind::Car | Kind::Train => vehicle(&mut d, &lay, &a, &tone, 0.2, 1.0),
                Kind::Boat => boat(&mut d, &lay, &a, &tone, 0.2, 1.0),
                Kind::Cat => cat(&mut d, &lay, &a, &tone, 0.2, 1.0),
                Kind::Bird => bird(&mut d, &lay, &a, &tone, 0.2, 1.0),
                Kind::Person => person(&mut d, &lay, &a, &tone, 0.2, 1.0),
            }
            assert!(cv.px.iter().any(|v| *v > 0.0), "{:?} drew nothing", kind);
        }
    }

    /// A contact sheet for looking at: not an assertion about pixels.
    #[test]
    fn contact_sheet_renders_when_asked() {
        let Ok(path) = std::env::var("DORMER_SHEET") else { return };
        let (w, h) = (960usize, 360usize);
        let lay = Layout::new(w, h, 0.7);
        let tone = Tone::new([70.0, 74.0, 86.0], 1.0, [150.0, 180.0, 220.0]);
        let mut cv = Canvas::new(w, h);
        cv.rows(&vec![[190.0, 200.0, 206.0]; h]);
        let mut sh = Shaper::new();
        let poses = [ActorPose::Idle, ActorPose::Look, ActorPose::Walk, ActorPose::Run, ActorPose::Carry, ActorPose::Operate, ActorPose::Work, ActorPose::Climb, ActorPose::React, ActorPose::Wave, ActorPose::Sit];
        let roles = [Role::Worker, Role::Roofer, Role::Plumber, Role::Police, Role::Courier, Role::Keeper, Role::Resident, Role::Electrician, Role::Dockhand, Role::Staff, Role::Customer];
        {
            let mut d = Draw::new(&mut cv, &mut sh);
            for (n, pose) in poses.iter().enumerate() {
                let a = actor(Kind::Person, roles[n], *pose, (60.0 + n as f32 * 130.0, 900.0), 1.7, n as u32);
                person(&mut d, &lay, &a, &tone, 0.4 + n as f32 * 0.13, 1.0);
            }
            for (n, (kind, role)) in [(Kind::Van, Role::Worker), (Kind::Truck, Role::Worker), (Kind::Car, Role::Police), (Kind::Boat, Role::Dockhand)].iter().enumerate() {
                let a = actor(*kind, *role, ActorPose::Idle, (160.0 + n as f32 * 340.0, 700.0), 1.5, n as u32);
                match kind {
                    Kind::Boat => boat(&mut d, &lay, &a, &tone, 0.2, 1.0),
                    _ => vehicle(&mut d, &lay, &a, &tone, 0.2, 1.0),
                }
            }
            let cat_a = actor(Kind::Cat, Role::Resident, ActorPose::Sit, (1500.0, 900.0), 1.7, 0);
            cat(&mut d, &lay, &cat_a, &tone, 0.2, 1.0);
            let cat_b = actor(Kind::Cat, Role::Resident, ActorPose::Walk, (1400.0, 900.0), 1.7, 1);
            cat(&mut d, &lay, &cat_b, &tone, 0.2, 1.0);
            ladder(&mut d, &lay, (1200.0, 700.0), (1260.0, 540.0), 1.5, &tone, 1.0);
            bucket(&mut d, &lay, (1320.0, 700.0), 1.7, &tone, 0.6, 1.0);
            cone(&mut d, &lay, (1380.0, 700.0), 1.7, &tone, 1.0);
            barrier(&mut d, &lay, (1460.0, 700.0), 90.0, 1.5, &tone, 1.0);
            crate_box(&mut d, &lay, (1540.0, 680.0), 1.5, 0.3, &tone, 1.0);
            parcel(&mut d, &lay, (1540.0, 720.0), 1.5, 0.2, 0, &tone, 1.0);
            toolbag(&mut d, &lay, (1100.0, 700.0), 1.7, &tone, 1.0);
            rope(&mut d, &lay, (1000.0, 600.0), (1100.0, 640.0), 12.0, 1.5, &tone, 1.0);
        }
        crate::gfx::save_png(&path, w, h, &cv.finish(0.0)).unwrap();
    }
}
