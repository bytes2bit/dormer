//! Painting with state: what is omitted shows what is really behind it, what is
//! replaced shows its damaged self at the same place, depth follows the state,
//! and installing only the changed rectangles is the same as painting it all.

use super::actors::{ActorPose, ActorSpec, ActorSystem, Kind, Mode, Role, RouteGraph};
use super::draw::{person, Tone};
use super::*;
use crate::buffer::{install_regions, restore_pixels};
use crate::color::mix;
use crate::gfx::{Bloom, Canvas, Depth, Draw, Frame, Rect, Shaper};
use crate::paint::{render, render_state, Cache};
use crate::props::Layout;
use crate::scene::Scene;
use crate::views::{Anchors, Lights, Stage, View};

const POLE: ObjectId = ObjectId(1);
const SIGN: ObjectId = ObjectId(2);
const LAMP: ObjectId = ObjectId(3);

/// A yard: a wall, a pole in front of it, a sign on the wall, a lamp that lights, and a sill.
struct Yard {
    lay: Layout,
    depth: Depth,
    w: usize,
    h: usize,
}

const POLE_X: (f32, f32) = (700.0, 740.0);
const SIGN_AT: (f32, f32) = (300.0, 400.0);
const LAMP_AT: (f32, f32) = (1000.0, 500.0);

impl Yard {
    fn new(w: usize, h: usize) -> Yard {
        let lay = Layout::new(w, h, 0.7);
        let mask = |with_pole: bool| -> Depth {
            let mut m = vec![0u8; w * h];
            if with_pole {
                let (x0, x1) = (lay.dx(POLE_X.0) as usize, lay.dx(POLE_X.1) as usize);
                for y in (lay.dy(200.0) as usize)..h {
                    for x in x0..x1.min(w) {
                        m[y * w + x] = 255;
                    }
                }
            }
            Depth::stack(&[(0, m)], w, h)
        };
        Yard { depth: mask(true), lay, w, h }
    }

    fn pole_depth(&self, with_pole: bool) -> Depth {
        let mut m = vec![0u8; self.w * self.h];
        if with_pole {
            let (x0, x1) = (self.lay.dx(POLE_X.0) as usize, self.lay.dx(POLE_X.1) as usize);
            for y in (self.lay.dy(200.0) as usize)..self.h {
                for x in x0..x1.min(self.w) {
                    m[y * self.w + x] = 255;
                }
            }
        }
        Depth::stack(&[(0, m)], self.w, self.h)
    }
}

impl Stage for Yard {
    fn lay(&self) -> &Layout {
        &self.lay
    }

    fn depth(&self) -> &Depth {
        &self.depth
    }

    fn lights(&self, _sc: &Scene) -> Lights {
        Lights { share: 0.0, on: Vec::new() }
    }

    fn ground(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors) {
        self.ground_state(d, sc, lights, bloom, out, &Snapshot::intact())
    }

    fn ground_state(&self, d: &mut Draw<Canvas>, _sc: &Scene, _l: &Lights, bloom: &mut Bloom, _out: &mut Anchors, state: &Snapshot) {
        let lay = &self.lay;
        // the wall, and the ground
        d.rect(0.0, lay.dy(300.0), lay.w, lay.h, [96.0, 100.0, 108.0], 1.0);
        d.rect(0.0, lay.dy(900.0), lay.w, lay.h, [70.0, 66.0, 60.0], 1.0);
        if state.in_base(POLE) {
            d.rect(lay.dx(POLE_X.0), lay.dy(200.0), lay.dx(POLE_X.1), lay.dy(900.0), [40.0, 40.0, 44.0], 1.0);
        }
        // the sign: on the wall, or hanging crooked by one fixing, or not there
        if state.in_base(SIGN) {
            let (x, y) = (lay.dx(SIGN_AT.0), lay.dy(SIGN_AT.1));
            let (w, h) = (lay.d(90.0), lay.d(40.0));
            match state.damage(SIGN) {
                Damage::Intact => d.rect(x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0, [200.0, 60.0, 50.0], 1.0),
                _ => d.poly(&[(x - w / 2.0, y - h / 2.0), (x + w / 2.0, y - h / 2.0 + h * 0.8), (x + w / 2.0, y + h / 2.0 + h * 0.8), (x - w / 2.0, y + h / 2.0)], [200.0, 60.0, 50.0], 1.0),
            }
        }
        // a lamp, with light round it
        if state.in_base(LAMP) {
            let (x, y) = (lay.dx(LAMP_AT.0), lay.dy(LAMP_AT.1));
            d.circle(x, y, lay.d(8.0), [250.0, 220.0, 150.0], 1.0);
            bloom.light(x - lay.d(10.0), y - lay.d(10.0), x + lay.d(10.0), y + lay.d(10.0), [255.0, 200.0, 120.0], 1.4);
        }
    }

    fn indoors(&self, d: &mut Draw<Canvas>, _sc: &Scene, _bloom: &mut Bloom, _out: &mut Anchors) {
        d.rect(0.0, self.lay.dy(940.0), self.lay.w, self.lay.h, [30.0, 28.0, 26.0], 1.0);
    }

    fn indoors_alive(&self, d: &mut Draw<Frame>, _sc: &Scene, _t: f32, _now: f64, _falling: bool) {
        d.s.behind = None;
        d.rect(0.0, self.lay.dy(940.0), self.lay.w, self.lay.h, [30.0, 28.0, 26.0], 1.0);
    }

    fn alive(&self, _d: &mut Draw<Frame>, _sc: &Scene, _a: &Anchors, _t: f32, _now: f64) {}

    fn story_depth(&self, state: &Snapshot) -> Option<Depth> {
        Some(self.pole_depth(state.in_base(POLE)))
    }

    fn story_alive(&self, d: &mut Draw<Frame>, _sc: &Scene, state: &Snapshot, _t: f32, indoors: bool) {
        if indoors {
            return;
        }
        // a sign that is swinging is drawn here, where the pose puts it
        let s = state.get(SIGN);
        if !state.in_base(SIGN) && s.visible && s.damage != Damage::Missing {
            let lay = &self.lay;
            let (x, y) = (lay.dx(SIGN_AT.0 + s.pose.dx), lay.dy(SIGN_AT.1 + s.pose.dy));
            d.s.behind = None;
            d.rect(x - lay.d(45.0), y - lay.d(20.0), x + lay.d(45.0), y + lay.d(20.0), [200.0, 60.0, 50.0], 1.0);
        }
    }
}

struct YardView;
static YARD: YardView = YardView;

impl View for YardView {
    fn id(&self) -> &'static str {
        "yard"
    }

    fn about(&self) -> &'static str {
        "a test yard"
    }

    fn stage(&self, _seed: u64, w: usize, h: usize) -> Box<dyn Stage> {
        Box::new(Yard::new(w, h))
    }

    fn words(&self, _sc: &Scene) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}

fn scene(rain: bool) -> Scene {
    let mut sc = crate::interaction::tests::scene(&YARD, rain);
    sc.live = false;
    sc
}

fn objects() -> Vec<ObjectDef> {
    vec![
        ObjectDef::new(1, "pole", Material::Metal, (720.0, 550.0), [PO.0, 200.0, PO.1, 900.0]),
        ObjectDef::new(2, "sign", Material::Metal, SIGN_AT, [SIGN_AT.0 - 50.0, SIGN_AT.1 - 25.0, SIGN_AT.0 + 50.0, SIGN_AT.1 + 70.0]),
        ObjectDef::new(3, "lamp", Material::Glass, LAMP_AT, [LAMP_AT.0 - 12.0, LAMP_AT.1 - 12.0, LAMP_AT.0 + 12.0, LAMP_AT.1 + 12.0]),
    ]
}
const PO: (f32, f32) = POLE_X;

fn world() -> StoryWorld {
    StoryWorld::new(Definition { view: "yard", objects: objects(), ..Default::default() }, 1).unwrap()
}

fn frame(cache: &Cache, state: &Snapshot) -> Vec<u32> {
    let mut sc = scene(false);
    let (cv, _) = render_state(&mut sc, cache, state);
    cv.finish(0.0)
}

fn px(p: &[u32], w: usize, x: f32, y: f32) -> u32 {
    p[y as usize * w + x as usize] & 0x00FF_FFFF
}

#[test]
fn an_omitted_prop_shows_the_real_background_and_nothing_else_changes() {
    let (w, h) = (640, 400);
    let cache = Cache::new(&YARD, 12, w, h);
    let lay = cache.lay;
    let mut story = world();
    let intact = frame(&cache, &story.snapshot());
    // the sign is taken away
    story.objects.get_mut(SIGN).unwrap().damage = Damage::Missing;
    let gone = frame(&cache, &story.snapshot());
    let at = (lay.dx(SIGN_AT.0), lay.dy(SIGN_AT.1));
    assert_ne!(px(&intact, w, at.0, at.1), px(&gone, w, at.0, at.1));
    // what shows is the wall behind it, the colour the wall is everywhere else
    let wall = px(&gone, w, lay.dx(100.0), lay.dy(SIGN_AT.1));
    assert_eq!(px(&gone, w, at.0, at.1), wall, "the wall is there, not an overpainted patch");
    // nothing differs outside the sign's bounds and a margin
    let area = pixel_rect(&DesignRect { x0: SIGN_AT.0 - 50.0, y0: SIGN_AT.1 - 25.0, x1: SIGN_AT.0 + 50.0, y1: SIGN_AT.1 + 70.0 }, &lay, LIGHT_MARGIN);
    for y in 0..h {
        for x in 0..w {
            if intact[y * w + x] != gone[y * w + x] {
                assert!(
                    (x as i32) >= area.x0 && (x as i32) < area.x1 && (y as i32) >= area.y0 && (y as i32) < area.y1,
                    "pixel {},{} changed outside {:?}",
                    x,
                    y,
                    area
                );
            }
        }
    }
}

#[test]
fn a_replaced_prop_draws_its_damaged_self_in_the_same_place() {
    let (w, h) = (640, 400);
    let cache = Cache::new(&YARD, 12, w, h);
    let mut story = world();
    let intact = frame(&cache, &story.snapshot());
    story.objects.get_mut(SIGN).unwrap().damage = Damage::Damaged;
    let damaged = frame(&cache, &story.snapshot());
    let lay = cache.lay;
    // the end by the fixing that still holds has not moved
    let corner = (lay.dx(SIGN_AT.0 - 30.0), lay.dy(SIGN_AT.1));
    assert_eq!(px(&intact, w, corner.0, corner.1), px(&damaged, w, corner.0, corner.1));
    // the far end has dropped: a pixel up there was sign and is now wall
    let low_right = (lay.dx(SIGN_AT.0 + 40.0), lay.dy(SIGN_AT.1 - 15.0));
    assert_ne!(px(&intact, w, low_right.0, low_right.1), px(&damaged, w, low_right.0, low_right.1));
    // a dragged sign is not in the base at all (the live layer draws it)
    story.objects.get_mut(SIGN).unwrap().attachment = Attachment::Dragged;
    let dragged = frame(&cache, &story.snapshot());
    assert_eq!(px(&dragged, w, low_right.0, low_right.1), px(&dragged, w, lay.dx(100.0), low_right.1));
}

#[test]
fn a_removed_pole_does_not_occlude_an_actor() {
    let (w, h) = (640, 400);
    let cache = Cache::new(&YARD, 12, w, h);
    let lay = cache.lay;
    let mut graph = RouteGraph::default();
    graph.add_node(1, (720.0, 880.0), Mode::Walk, 1.2);
    let mut actors = ActorSystem::new();
    let id = actors.spawn(ActorSpec { kind: Kind::Person, role: Role::Worker, at: RouteId(1), look: 0 }, &graph).unwrap();
    let mut a = actors.get(id).unwrap().clone();
    a.pose = ActorPose::Idle;
    a.layer = Some(0);
    let tone = Tone::new([80.0, 80.0, 90.0], 1.0, [150.0, 170.0, 200.0]);
    let draw_with = |depth: &Depth| -> Vec<u32> {
        let mut pixels = frame(&cache, &Snapshot::intact());
        let mut dirty = Vec::new();
        let mut sh = Shaper::new();
        let mut f = Frame { w, h, px: &mut pixels, dirty: &mut dirty, depth: Some(depth), behind: Some(0), full_damage: false };
        let mut d = Draw::new(&mut f, &mut sh);
        person(&mut d, &lay, &a, &tone, 0.2, 1.0);
        pixels
    };
    let before = frame(&cache, &Snapshot::intact());
    // with the pole standing, someone standing in line with it is hidden by it
    let hidden = draw_with(cache.stage.depth());
    let pole_pixels = (lay.dx(POLE_X.0) as usize + 2..lay.dx(POLE_X.1) as usize - 2).flat_map(|x| (lay.dy(840.0) as usize..lay.dy(895.0) as usize).map(move |y| (x, y)));
    let changed_hidden = pole_pixels.clone().filter(|(x, y)| hidden[y * w + x] != before[y * w + x]).count();
    assert_eq!(changed_hidden, 0, "the pole hides them");
    // the pole is gone: so is the occlusion
    let mut story = world();
    story.objects.get_mut(POLE).unwrap().damage = Damage::Missing;
    let depth = cache.stage.story_depth(&story.snapshot()).expect("a stage with a pole reports its depth");
    let shown = draw_with(&depth);
    let changed_shown = pole_pixels.filter(|(x, y)| shown[y * w + x] != before[y * w + x]).count();
    assert!(changed_shown > 20, "{} pixels of the actor show where the pole was", changed_shown);
    // and the base itself no longer has a pole under them
    let base = frame(&cache, &story.snapshot());
    assert_ne!(base[lay.dy(500.0) as usize * w + lay.dx(720.0) as usize], before[lay.dy(500.0) as usize * w + lay.dx(720.0) as usize]);
}

#[test]
fn sparse_installation_equals_a_full_repaint_and_leaves_no_ghosts() {
    let (w, h) = (640, 400);
    let cache = Cache::new(&YARD, 12, w, h);
    let lay = cache.lay;
    let mut story = world();
    let mut shown = frame(&cache, &story.snapshot());
    // the lamp is smashed, the sign hangs crooked: two separate things change
    story.objects.get_mut(LAMP).unwrap().damage = Damage::Missing;
    story.objects.get_mut(SIGN).unwrap().damage = Damage::Damaged;
    let want = frame(&cache, &story.snapshot());
    let regions: Vec<Rect> = [LAMP, SIGN]
        .iter()
        .map(|id| {
            let d = story.definition().object(*id).unwrap();
            pixel_rect(&DesignRect { x0: d.bounds[0], y0: d.bounds[1], x1: d.bounds[2], y1: d.bounds[3] }, &lay, LIGHT_MARGIN)
        })
        .collect();
    let written = install_regions(&mut shown, &want, w, &regions);
    assert_eq!(shown, want, "installing the two regions is the same picture as painting it all");
    assert!(written < w * h / 4, "and it touched {} of {} pixels", written, w * h);
    // two buffers, each with an old drawing on it: both end up with exactly the base
    let mut buf = [shown.clone(), shown.clone()];
    let mut marks: [Vec<Rect>; 2] = [vec![Rect { x0: 100, y0: 100, x1: 140, y1: 130 }], vec![Rect { x0: 200, y0: 50, x1: 260, y1: 90 }]];
    // each has had something drawn on it at its own place
    for (k, b) in buf.iter_mut().enumerate() {
        for r in &marks[k] {
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    b[y as usize * w + x as usize] = 0xFFFF_00FF;
                }
            }
        }
    }
    for b in 0..2 {
        restore_pixels(&mut buf[b], &want, w, None, &marks[b]);
        marks[b].clear();
        assert_eq!(buf[b], want, "buffer {} has no ghost of what was drawn on it", b);
    }
}

#[test]
fn rain_falls_behind_what_is_indoors() {
    let (w, h) = (640, 400);
    let cache = Cache::new(&YARD, 12, w, h);
    let mut sc = scene(true);
    sc.live = true;
    let (cv, anchors) = render(&mut sc, &cache);
    let mut pixels = cv.finish(0.0);
    let mut dirty = Vec::new();
    let mut shaper = Shaper::new();
    let mut live = crate::live::World::new(5);
    live.retune(&sc, 0);
    for _ in 0..30 {
        live.step(0.05, &sc, &crate::live::Sources::from(&anchors, &cache.lay), sc.ts);
    }
    let mut f = Frame { w, h, px: &mut pixels, dirty: &mut dirty, depth: Some(cache.stage.depth()), behind: None, full_damage: true };
    let mut d = Draw::new(&mut f, &mut shaper);
    live.draw(&mut d, &sc, &cache, &anchors, sc.ts, None);
    // the sill band is exactly its own colour, undisturbed by the rain
    let y = cache.lay.dy(960.0) as usize;
    let want = 0x1E1C1A;
    let off = (0..w).filter(|x| pixels[y * w + x] & 0x00FF_FFFF != want).count();
    assert_eq!(off, 0, "{} pixels of the sill were rained on", off);
    // above it, the rain is there
    let sky_row = cache.lay.dy(500.0) as usize;
    let streaks = (0..w).map(|x| pixels[sky_row * w + x]).collect::<std::collections::HashSet<_>>().len();
    assert!(streaks > 4, "rain falls in the yard");
}

#[test]
fn a_scene_with_no_changes_paints_exactly_what_render_does() {
    let cache = Cache::new(&YARD, 12, 320, 200);
    let story = world();
    let mut a = scene(false);
    let mut b = scene(false);
    let (cv_a, _) = render(&mut a, &cache);
    let (cv_b, _) = render_state(&mut b, &cache, &story.snapshot());
    assert_eq!(cv_a.finish(1.0), cv_b.finish(1.0));
    let _ = mix([0.0; 3], [1.0; 3], 0.5);
}

