//! Every principal incident of every view, run repeatedly and in different air, must come back
//! to a world that is the one it started from: nothing left claimed, held, hidden or broken, no
//! one still standing about, no growth.

use super::director::Class;
use super::model::{Attachment, Damage, ObjectDef, Pose};
use super::replay::scripted::*;
use super::replay::{test_session, Session};
use crate::views::ALL;

/// Is the object as the view authored it?
fn as_authored(s: &Session, def: &ObjectDef) -> bool {
    let snap = s.story.snapshot();
    let o = snap.get(def.id);
    // a prop that is not out is put away where it was left; every story that brings it out
    // puts it where it wants it first, so where it lies while away is nobody's business
    let place = def.hidden || o.pose == Pose::default();
    o.damage == Damage::Intact && place && o.attachment == Attachment::Fixed && o.visible == !def.hidden && !o.open && o.owner.is_none()
}

/// Run one incident through to its end, up to its own limit plus a margin.
fn run_incident(s: &mut Session, id: super::StoryId, limit: f32) {
    assert_eq!(s.story.trigger(id), super::TriggerResult::Started, "{:?} did not start", id);
    let done = until(s, limit as f64 + 90.0, |s| !s.story.director().active() && s.story.actors().reservation_count() == 0);
    assert!(done.is_some(), "story {:?} never finished", id);
}

fn cycle_all(view: &str, at: &str, weather: &str, size: (usize, usize), cycles: usize) {
    let mut s = test_session(view, at, weather, size);
    run(&mut s, 1.0);
    let incidents: Vec<(super::StoryId, &'static str, f32)> =
        s.story.definition().stories.iter().filter(|d| d.class == Class::Incident).map(|d| (d.id, d.name, d.limit)).collect();
    assert!(!incidents.is_empty() || view == "moon", "{} has no incident", view);
    let mut most_bodies = 0;
    for cycle in 0..cycles {
        for (id, name, limit) in &incidents {
            run_incident(&mut s, *id, *limit);
            // give the last departures a moment, and the ambient world its own
            run(&mut s, 20.0);
            let defs = s.story.definition().clone();
            for def in &defs.objects {
                assert!(as_authored(&s, def), "{} {} cycle {} in {}: {} is not as authored: {:?}", view, name, cycle, weather, def.name, s.story.snapshot().get(def.id));
            }
            quiet(&s);
            most_bodies = most_bodies.max(s.story.physics().bodies.len());
            assert!(s.story.actors().count() <= 24);
        }
    }
    assert!(most_bodies < 40, "{}: bodies grew to {}", view, most_bodies);
}

#[test]
fn every_incident_returns_the_world_to_where_it_began_over_repeated_cycles() {
    for view in ALL.iter().map(|v| v.id()) {
        if view == "moon" {
            continue;
        }
        cycle_all(view, "2026-10-13 15:30", "clear", (640, 400), 3);
    }
}

#[test]
fn the_same_by_night_in_the_rain_and_at_other_proportions() {
    for view in ALL.iter().map(|v| v.id()) {
        if view == "moon" {
            continue;
        }
        cycle_all(view, "2026-10-13 21:00", "rain", (900, 900), 2);
        cycle_all(view, "2026-10-13 07:10", "snow", (1280, 400), 1);
    }
}

#[test]
fn an_incident_that_is_pestered_ends_in_a_protected_cleanup_that_leaves_nothing() {
    for view in ALL.iter().map(|v| v.id()) {
        if view == "moon" {
            continue;
        }
        let mut s = test_session(view, "2026-10-13 15:30", "clear", (640, 400));
        run(&mut s, 1.0);
        let incidents: Vec<(super::StoryId, &'static str, f32, Vec<super::ObjectId>)> =
            s.story.definition().stories.iter().filter(|d| d.class == Class::Incident).map(|d| (d.id, d.name, d.limit, d.interference.clone())).collect();
        for (id, name, limit, objects) in incidents {
            assert_eq!(s.story.trigger(id), super::TriggerResult::Started, "{} {}", view, name);
            let target = objects.first().copied();
            for n in 0..20u64 {
                if let Some(o) = target {
                    s.story.bus_emit_impact(o, 40_000 + n);
                }
                run(&mut s, 3.0);
            }
            let done = until(&mut s, limit as f64 + 120.0, |s| !s.story.director().active() && s.story.actors().reservation_count() == 0);
            assert!(done.is_some(), "{} {} never ended", view, name);
            run(&mut s, 20.0);
            let defs = s.story.definition().clone();
            for def in &defs.objects {
                assert!(as_authored(&s, def), "{} {} pestered: {} is not as authored: {:?}", view, name, def.name, s.story.snapshot().get(def.id));
            }
            quiet(&s);
        }
    }
}
