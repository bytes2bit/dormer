//! The director and the shared behaviors, run against small fixture views.

use super::actors::{Kind, Mode, Role, RouteGraph};
use super::behaviors::{repair, Animal, BehaviorSpec, CrewJob, Perch};
use super::director::{Class, Cond, MemoryKey, Slot, Step, StoryDef, Test, Trigger, Until, Weather};
use super::events::{Event, EventKind};
use super::physics::{Segment, SurfaceMap};
use super::*;

const WHO: Slot = Slot(0);

fn objects() -> Vec<ObjectDef> {
    vec![
        ObjectDef::new(1, "cap", Material::Metal, (500.0, 300.0), [480.0, 280.0, 520.0, 320.0]).accepting(Accepts::POKE.or(Accepts::GRAB)).breaks_at(30.0),
        ObjectDef::new(2, "bucket", Material::Metal, (300.0, 500.0), [280.0, 480.0, 320.0, 520.0]).accepting(Accepts::MOVE.or(Accepts::THROW)),
        ObjectDef::new(3, "tank", Material::Metal, (520.0, 360.0), [490.0, 330.0, 550.0, 380.0]),
    ]
}

/// A fixture view: a depot, a site by the cap, an exit; and a roof for a cat and birds.
fn routes(site_x: f32) -> RouteGraph {
    let mut g = RouteGraph::default();
    g.add_node(1, (0.0, 600.0), Mode::Walk, 1.0)
        .add_node(2, (site_x, 600.0), Mode::Walk, 1.0)
        .add_node(3, (1500.0, 600.0), Mode::Walk, 1.0)
        .add_node(10, (200.0, 200.0), Mode::Roof, 0.5)
        .add_node(11, (260.0, 200.0), Mode::Roof, 0.5)
        .add_node(12, (800.0, 200.0), Mode::Roof, 0.5)
        .add_node(20, (700.0, 600.0), Mode::Walk, 1.0)
        .add_node(99, (-300.0, 700.0), Mode::Water, 1.0); // somewhere nobody can walk to
    g.connect(1, 2, Mode::Walk).connect(2, 3, Mode::Walk).connect(2, 20, Mode::Walk);
    g.connect(10, 11, Mode::Roof).connect(11, 12, Mode::Roof);
    g
}

fn crew(depot: u16, site: u16) -> CrewJob {
    CrewJob {
        who: WHO,
        role: Role::Plumber,
        kind: Kind::Person,
        look: 1,
        depot: RouteId(depot),
        site: RouteId(site),
        exit: RouteId(3),
        area: AreaId(1),
        target: ObjectId(1),
        work_secs: 8.0,
        tool: None,
        patience: 60.0,
    }
}

/// "A cap breaks; a crew comes, mends it and goes", with the stages every incident here shares.
fn cap_story(job: CrewJob) -> StoryDef {
    let mut story = StoryDef::new(1, "cap", Class::Incident, Trigger::Damage(ObjectId(1), Damage::Broken))
        .disturbed_by(&[ObjectId(1)])
        .escalating(&["more", "more_again", "most"])
        .helped_by(ObjectId(2), ObjectId(1), 80.0)
        .within(120.0)
        .stage("start", vec![Step::Wait(1.0), Step::Goto("arrive")])
        .stage("more", vec![Step::Wait(1.0), Step::Goto("arrive")])
        .stage("more_again", vec![Step::Wait(1.0), Step::Goto("arrive")])
        .stage("most", vec![Step::Wait(1.0), Step::Goto("arrive")])
        .stage("cleanup", vec![Step::Release { who: WHO }, Step::SetState { obj: ObjectId(1), change: Change::Restore }, Step::Goto("depart")]);
    story.stages.extend(repair(["arrive", "work", "depart", "fallback"], "done", &job));
    story.stage("done", vec![Step::Remember(MemoryKey::Repairs), Step::End])
}

fn definition(view: &'static str, site_x: f32, job: CrewJob) -> Definition {
    Definition {
        view,
        objects: objects(),
        routes: routes(site_x),
        surfaces: SurfaceMap { segments: vec![Segment { a: (0.0, 700.0), b: (1600.0, 700.0), material: Material::Stone }], ..Default::default() },
        stories: vec![cap_story(job)],
        behaviors: vec![
            BehaviorSpec::Perch(Perch { perches: vec![RouteId(10), RouteId(11)], alternates: vec![RouteId(12)], radius: 120.0, away: 5.0, keep_off: Some(AreaId(1)), shy_of: vec![] }),
            BehaviorSpec::Animal(Animal { kind: Kind::Cat, home: RouteId(10), haunts: vec![RouteId(11)], attention: 900.0, look: 0, linger: 3.0 }),
        ],
        ..Default::default()
    }
}

fn fair() -> Facts {
    Facts { hour: 12.0, wind: 5.0, weekday: true, ..Default::default() }
}

fn run(w: &mut StoryWorld, seconds: f64, facts: &Facts) {
    let mut t = 0.0;
    while t < seconds {
        w.step_facts(0.05, facts);
        t += 0.05;
    }
}

fn break_cap(w: &mut StoryWorld) {
    w.objects.get_mut(ObjectId(1)).unwrap().damage = Damage::Broken;
}

#[test]
fn a_broken_cap_is_mended_by_a_crew_that_comes_and_goes() {
    let mut w = StoryWorld::new(definition("fixture-a", 500.0, crew(1, 2)), 1).unwrap();
    run(&mut w, 2.0, &fair());
    break_cap(&mut w);
    run(&mut w, 2.5, &fair());
    assert_eq!(w.director().incidents_running(w.definition()), 1);
    assert!(w.actors().iter().any(|a| a.role == Role::Plumber), "the crew has arrived on the scene");
    run(&mut w, 60.0, &fair());
    assert_eq!(w.snapshot().damage(ObjectId(1)), Damage::Intact);
    assert_eq!(w.director().run_count(), 0);
    assert!(w.actors().iter().all(|a| a.role != Role::Plumber), "and gone again");
    assert_eq!(w.actors().reservation_count(), 0);
    assert!(w.director().memory.repairs >= 1);
}

#[test]
fn guarded_branches_and_weighted_variants() {
    let mut def = definition("fixture-a", 500.0, crew(1, 2));
    def.stories.push(
        StoryDef::new(2, "pick", Class::Incident, Trigger::Named)
            .stage("start", vec![
                Step::Branch { when: Cond::is(Test::Weather(Weather::Rain)), then: "wet", otherwise: "dry" },
            ])
            .stage("wet", vec![Step::SetState { obj: ObjectId(3), change: Change::Damage(Damage::Damaged) }, Step::End])
            .stage("dry", vec![Step::Choose(vec![(1, "a"), (3, "b")])])
            .stage("a", vec![Step::SetState { obj: ObjectId(3), change: Change::Open(true) }, Step::End])
            .stage("b", vec![Step::SetState { obj: ObjectId(3), change: Change::Place((560.0, 360.0)) }, Step::End])
            .stage("cleanup", vec![Step::End]),
    );
    // guarded: rain takes one way, dry the other
    let mut w = StoryWorld::new(def.clone(), 5).unwrap();
    let wet = Facts { rain: true, ..fair() };
    assert_eq!(w.trigger(StoryId(2)), TriggerResult::Started);
    run(&mut w, 0.2, &wet);
    assert_eq!(w.snapshot().damage(ObjectId(3)), Damage::Damaged);
    // weighted, deterministic: the same seed picks the same variant, and over many seeds the
    // heavier one is picked about three times in four
    let pick = |seed: u64| {
        let mut w = StoryWorld::new(def.clone(), seed).unwrap();
        w.trigger(StoryId(2));
        run(&mut w, 0.2, &fair());
        w.snapshot().get(ObjectId(3)).open
    };
    assert_eq!(pick(11), pick(11));
    let a = (0..400).filter(|s| pick(*s)).count();
    assert!((60..140).contains(&a), "the light option came up {} times in 400", a);
}

#[test]
fn await_succeeds_when_it_holds_and_times_out_when_it_does_not() {
    let mut def = definition("fixture-a", 500.0, crew(1, 2));
    def.stories.push(
        StoryDef::new(3, "wait", Class::Incident, Trigger::Named)
            .stage("start", vec![
                Step::Await { until: Until::ObjectIs(ObjectId(3), Damage::Broken), timeout: 3.0, on_timeout: Some("late") },
                Step::SetState { obj: ObjectId(2), change: Change::Open(true) },
                Step::End,
            ])
            .stage("late", vec![Step::SetState { obj: ObjectId(2), change: Change::Damage(Damage::Damaged) }, Step::End])
            .stage("cleanup", vec![Step::End]),
    );
    // it holds in time
    let mut w = StoryWorld::new(def.clone(), 1).unwrap();
    w.trigger(StoryId(3));
    run(&mut w, 1.0, &fair());
    w.objects.get_mut(ObjectId(3)).unwrap().damage = Damage::Broken;
    run(&mut w, 1.0, &fair());
    assert!(w.snapshot().get(ObjectId(2)).open && w.snapshot().damage(ObjectId(2)) == Damage::Intact);
    // it does not: the timeout branch runs, and the story still ends
    let mut w = StoryWorld::new(def, 1).unwrap();
    w.trigger(StoryId(3));
    run(&mut w, 5.0, &fair());
    assert_eq!(w.snapshot().damage(ObjectId(2)), Damage::Damaged);
    assert_eq!(w.director().run_count(), 0);
}

#[test]
fn exactly_three_escalations_then_a_protected_cleanup_that_cannot_be_restarted() {
    let mut w = StoryWorld::new(definition("fixture-a", 500.0, crew(1, 2)), 2).unwrap();
    break_cap(&mut w);
    run(&mut w, 1.0, &fair());
    let defs = w.definition().clone();
    let mut seen: Vec<u8> = Vec::new();
    let mut crew_peak = 0;
    // the user keeps at it, a bang every three seconds, for a minute and a half
    for n in 0..30 {
        let mut e = Event::new(EventKind::Impact, ObjectId(1), (500.0, 300.0), 7000 + n);
        e.tags = 0;
        w.bus.emit(e);
        run(&mut w, 3.0, &fair());
        for (_, _, escalations, protected) in w.director().status(&defs) {
            seen.push(escalations);
            if protected {
                assert!(escalations <= 3);
            }
        }
        crew_peak = crew_peak.max(w.actors().iter().filter(|a| a.role == Role::Plumber).count());
    }
    assert!(seen.iter().all(|e| *e <= 3), "never more than three escalations: {:?}", seen);
    assert!(seen.contains(&3), "it did escalate that far");
    assert!(crew_peak <= 1, "the repair crew did not grow: {}", crew_peak);
    // and it came to an end regardless, with nothing left behind
    run(&mut w, 120.0, &fair());
    assert_eq!(w.director().run_count(), 0, "{:?}", w.director().status(&defs));
    assert_eq!(w.snapshot().damage(ObjectId(1)), Damage::Intact);
    assert_eq!(w.actors().reservation_count(), 0);
    assert!(w.actors().iter().all(|a| a.role != Role::Plumber));
}

#[test]
fn failed_and_cancelled_jobs_release_what_they_hold() {
    // nobody can reach the site: the depot is across water
    let mut w = StoryWorld::new(definition("fixture-b", 500.0, crew(99, 2)), 3).unwrap();
    break_cap(&mut w);
    run(&mut w, 8.0, &fair());
    // the stand-in completes the job rather than leaving it half done
    assert_eq!(w.snapshot().damage(ObjectId(1)), Damage::Intact);
    assert_eq!(w.director().run_count(), 0);
    assert!(w.actors().iter().all(|a| a.role != Role::Plumber));
    assert_eq!(w.actors().reservation_count(), 0);

    // cancelled in the middle of the work: reset
    let mut w = StoryWorld::new(definition("fixture-a", 500.0, crew(1, 2)), 3).unwrap();
    break_cap(&mut w);
    run(&mut w, 20.0, &fair());
    assert!(w.actors().iter().any(|a| a.role == Role::Plumber));
    assert_eq!(w.actors().reservation_count(), 1, "the crew holds its area while it works");
    w.reset();
    assert_eq!(w.actors().iter().filter(|a| a.role == Role::Plumber).count(), 0);
    assert_eq!(w.actors().reservation_count(), 0);
    assert_eq!(w.snapshot().damage(ObjectId(1)), Damage::Intact);
}

#[test]
fn the_instruction_budget_yields_after_256_steps_and_carries_on() {
    let mut steps: Vec<Step> = (0..600).map(|_| Step::SetState { obj: ObjectId(3), change: Change::Open(true) }).collect();
    steps.push(Step::SetState { obj: ObjectId(2), change: Change::Open(true) });
    steps.push(Step::End);
    let mut def = definition("fixture-a", 500.0, crew(1, 2));
    def.stories.push(StoryDef::new(4, "long", Class::Incident, Trigger::Named).stage("start", steps).stage("cleanup", vec![Step::End]));
    let mut w = StoryWorld::new(def, 1).unwrap();
    w.trigger(StoryId(4));
    w.step_facts(0.034, &fair()); // one tick
    assert!(!w.snapshot().get(ObjectId(2)).open, "not done in one tick");
    assert!(w.director().peak_instructions <= 256, "{}", w.director().peak_instructions);
    for _ in 0..4 {
        w.step_facts(0.034, &fair());
    }
    assert!(w.snapshot().get(ObjectId(2)).open, "done by the third tick");
    assert!(w.director().peak_instructions <= 256);
}

#[test]
fn loops_that_never_wait_and_waits_with_no_end_are_refused() {
    let mut def = definition("fixture-a", 500.0, crew(1, 2));
    def.stories.push(
        StoryDef::new(5, "spin", Class::Incident, Trigger::Named)
            .stage("start", vec![Step::Goto("again")])
            .stage("again", vec![Step::SetState { obj: ObjectId(3), change: Change::Open(true) }, Step::Goto("start")])
            .stage("cleanup", vec![Step::End]),
    );
    let e = StoryWorld::new(def, 1).err().expect("refused");
    assert_eq!(e.view, "fixture-a");
    assert_eq!(e.story.as_deref(), Some("spin"));
    assert!(e.why.contains("loop that never waits"), "{}", e);
    // the same loop with a wait in it is fine
    let mut ok = definition("fixture-a", 500.0, crew(1, 2));
    ok.stories.push(
        StoryDef::new(5, "pulse", Class::Incident, Trigger::Named)
            .stage("start", vec![Step::Goto("again")])
            .stage("again", vec![Step::SetState { obj: ObjectId(3), change: Change::Open(true) }, Step::Wait(1.0), Step::Goto("start")])
            .within(10.0)
            .stage("cleanup", vec![Step::End]),
    );
    assert!(StoryWorld::new(ok.clone(), 1).is_ok());
    // a wait must have a timeout; a jump must land; a place must exist
    let mut bad = definition("fixture-a", 500.0, crew(1, 2));
    bad.stories.push(StoryDef::new(6, "endless", Class::Incident, Trigger::Named).stage("start", vec![Step::Await { until: Until::Assisted, timeout: f32::INFINITY, on_timeout: None }]).stage("cleanup", vec![Step::End]));
    assert!(StoryWorld::new(bad, 1).err().unwrap().why.contains("timeout"));
    let mut bad = definition("fixture-a", 500.0, crew(1, 2));
    bad.stories.push(StoryDef::new(6, "lost", Class::Incident, Trigger::Named).stage("start", vec![Step::Goto("nowhere")]).stage("cleanup", vec![Step::End]));
    assert!(StoryWorld::new(bad, 1).err().unwrap().why.contains("not a stage"));
    let mut bad = definition("fixture-a", 500.0, crew(1, 2));
    bad.stories.push(StoryDef::new(6, "far", Class::Incident, Trigger::Named).stage("start", vec![Step::Move { who: WHO, to: RouteId(404), run: false }, Step::End]).stage("cleanup", vec![Step::End]));
    assert!(StoryWorld::new(bad, 1).err().unwrap().why.contains("routes"));
}

#[test]
fn a_hard_limit_forces_the_cleanup_of_a_story_that_will_not_finish() {
    let mut def = definition("fixture-a", 500.0, crew(1, 2));
    def.stories.push(
        StoryDef::new(7, "stubborn", Class::Incident, Trigger::Named)
            .within(10.0)
            .stage("start", vec![Step::Wait(5.0), Step::Goto("start")])
            .stage("cleanup", vec![Step::SetState { obj: ObjectId(2), change: Change::Open(true) }, Step::End]),
    );
    let mut w = StoryWorld::new(def, 1).unwrap();
    w.trigger(StoryId(7));
    run(&mut w, 8.0, &fair());
    assert_eq!(w.director().run_count(), 1);
    run(&mut w, 10.0, &fair());
    assert_eq!(w.director().run_count(), 0, "forced to its end");
    assert!(w.snapshot().get(ObjectId(2)).open, "the cleanup stage ran");
}

#[test]
fn the_same_repair_and_animal_behaviors_work_in_two_different_views() {
    for (view, site_x, depot) in [("fixture-a", 500.0, 1), ("fixture-b", 1100.0, 3)] {
        let mut job = crew(depot, 2);
        job.exit = RouteId(1);
        let mut w = StoryWorld::new(definition(view, site_x, job), 4).unwrap();
        run(&mut w, 3.0, &fair());
        // the cat and the birds are there
        assert!(w.actors().iter().any(|a| a.kind == Kind::Cat), "{}", view);
        assert!(w.actors().iter().filter(|a| a.kind == Kind::Bird).count() == 2);
        break_cap(&mut w);
        w.bus.emit(Event::new(EventKind::Damage, ObjectId(1), (230.0, 210.0), 99));
        run(&mut w, 13.0, &fair());
        // the birds left their perch for the other roof
        let on_alt = w.actors().iter().filter(|a| a.kind == Kind::Bird && (a.pos.0 - 800.0).abs() < 20.0).count();
        assert_eq!(on_alt, 2, "{}: birds went to the alternate", view);
        run(&mut w, 120.0, &fair());
        assert_eq!(w.snapshot().damage(ObjectId(1)), Damage::Intact, "{}", view);
        assert_eq!(w.director().run_count(), 0);
        // the site is out of bounds only while it is held: the birds are home again
        let home = w.actors().iter().filter(|a| a.kind == Kind::Bird && a.pos.0 < 300.0).count();
        assert_eq!(home, 2, "{}: birds back", view);
    }
}

#[test]
fn many_cycles_leave_no_actors_reservations_or_growth_behind() {
    let mut w = StoryWorld::new(definition("fixture-a", 500.0, crew(1, 2)), 9).unwrap();
    run(&mut w, 2.0, &fair());
    let baseline = w.actors().count();
    for cycle in 0..25 {
        break_cap(&mut w);
        run(&mut w, 80.0, &fair());
        assert_eq!(w.director().run_count(), 0, "cycle {}", cycle);
        assert_eq!(w.actors().count(), baseline, "cycle {}", cycle);
        assert_eq!(w.actors().reservation_count(), 0);
        assert!(w.physics().bodies.len() < 16);
        assert_eq!(w.snapshot().damage(ObjectId(1)), Damage::Intact);
    }
}

#[test]
fn a_returned_part_is_a_helping_hand_and_shortens_the_work() {
    let finish_time = |help: bool| {
        let mut w = StoryWorld::new(definition("fixture-a", 500.0, crew(1, 2)), 6).unwrap();
        break_cap(&mut w);
        let mut t = 0.0;
        let mut helped = false;
        while w.snapshot().damage(ObjectId(1)) != Damage::Intact && t < 100.0 {
            w.step_facts(0.05, &fair());
            t += 0.05;
            // once the plumber is at work, the user puts the bucket down next to the cap
            if help && !helped && w.actors().iter().any(|a| a.role == Role::Plumber && a.pose == actors::ActorPose::Work) {
                w.objects.get_mut(ObjectId(2)).unwrap().pose = Pose { dx: 190.0, dy: -190.0, turn: 0.0 };
                w.bus.emit(Event::new(EventKind::Obstruction, ObjectId(2), (490.0, 310.0), 4242));
                helped = true;
            }
        }
        t
    };
    let alone = finish_time(false);
    let helped = finish_time(true);
    assert!(helped + 3.0 < alone, "helped {} vs alone {}", helped, alone);
}

#[test]
fn nothing_is_polling_while_the_scene_sleeps() {
    let mut w = StoryWorld::new(definition("fixture-a", 500.0, crew(1, 2)), 1).unwrap();
    // birds and a cat settle in...
    run(&mut w, 4.0, &fair());
    // ...and then, with nothing happening, there is nothing to draw and nothing to wake for
    assert!(w.physics().bodies.iter().all(|b| b.asleep));
    run(&mut w, 30.0, &fair());
    assert!(!w.director().active());
    assert_eq!(w.next_deadline(), None, "no timer is due for sleeping behaviors");
}
