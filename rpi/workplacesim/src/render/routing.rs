//! Corridor routing. Ports `public/main.js` `computeRoute`, `pathFromDoorTo`,
//! `pathBetweenHallNodes`, and helpers. Outputs an ordered polyline; sims
//! tween between consecutive points.
//!
//! Shape: vertical transit hugs `HALLWAY_LEFT_X` / `HALLWAY_RIGHT_X`,
//! horizontal transit stays on a `CORRIDOR_YS` lane so the path never cuts
//! through a desk. The only non-corridor horizontal segments are stubs at
//! either end (entering/leaving the hallway) that sit in doors or inside a
//! room.

use super::geometry::{
    DeskSeat, LabStation, MeetingSeat, Point, CORRIDOR_YS, DOOR, HALLWAY_LEFT_X, HALLWAY_RIGHT_X,
    LAB_DOOR, LAB_ROOM, MEETING_DOOR, MEETING_ROOM, OPEN_ROOM, OUTSIDE_X,
};

/// Snap an arbitrary y to the nearest corridor-y. Ties go to the first
/// encountered — matching JS's strict `<` comparison in the reduction.
pub fn nearest_corridor_y(y: i32) -> i32 {
    let mut best = CORRIDOR_YS[0];
    let mut best_d = (y - best).abs();
    for &cy in &CORRIDOR_YS {
        let d = (y - cy).abs();
        if d < best_d {
            best_d = d;
            best = cy;
        }
    }
    best
}

pub fn on_corridor(y: i32) -> bool {
    CORRIDOR_YS.contains(&y)
}

/// What kind of target a sim is walking to. Mirrors the JS `target` object
/// shapes 1:1, including the "desk" vs "queue" (overflow) split and the
/// meeting/lab variants with or without an assigned seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Desk(DeskSeat),
    Queue(Point),
    Meeting(MeetingSeat),
    MeetingQueue(Point),
    Lab(LabStation),
    LabQueue(Point),
}

/// The hallway handoff point where cross-hall transit starts. Mirrors
/// `stagingForTarget` in JS.
pub fn staging_for_target(target: &Target) -> Point {
    match target {
        Target::Desk(d) => Point::new(HALLWAY_LEFT_X, d.approach_y),
        Target::Queue(p) => Point::new(HALLWAY_RIGHT_X, nearest_corridor_y(p.y)),
        Target::Meeting(_) | Target::MeetingQueue(_) => {
            Point::new(HALLWAY_RIGHT_X, nearest_corridor_y(MEETING_DOOR.y))
        }
        Target::Lab(_) | Target::LabQueue(_) => {
            Point::new(HALLWAY_RIGHT_X, nearest_corridor_y(LAB_DOOR.y))
        }
    }
}

/// Connect two hall nodes (any pair of `{HALLWAY_LEFT_X, HALLWAY_RIGHT_X} ×
/// corridor-y`). Same-column is straight down/up; cross-column snaps the
/// horizontal leg to a corridor-y so it never crosses a desk row.
///
/// JS quirk preserved: the second `if` always pushes because we early-returned
/// the `from.x === to.x` case. Porting the condition verbatim for parity.
pub fn path_between_hall_nodes(from: Point, to: Point) -> Vec<Point> {
    if from == to {
        return vec![];
    }
    if from.x == to.x {
        return vec![to];
    }
    let transit_y = if on_corridor(from.y) {
        from.y
    } else {
        nearest_corridor_y(from.y)
    };
    let mut out = Vec::new();
    if transit_y != from.y {
        out.push(Point::new(from.x, transit_y));
    }
    if transit_y != to.y || from.x != to.x {
        out.push(Point::new(to.x, transit_y));
    }
    if transit_y != to.y {
        out.push(to);
    }
    out
}

/// Approach waypoints from the staging point to the final seat/queue spot.
/// Mirrors JS `targetApproachWaypoints`.
pub fn target_approach_waypoints(target: &Target) -> Vec<Point> {
    match target {
        Target::Desk(d) => vec![
            Point::new(d.approach_x, d.approach_y),
            Point::new(d.seat_x, d.seat_y),
        ],
        Target::Queue(p) => vec![*p],
        Target::Meeting(s) => vec![
            Point::new(HALLWAY_RIGHT_X, MEETING_DOOR.y),
            Point::new(MEETING_ROOM.x + 24, MEETING_DOOR.y),
            Point::new(s.approach_x, s.approach_y),
            Point::new(s.seat_x, s.seat_y),
        ],
        Target::MeetingQueue(p) => vec![
            Point::new(HALLWAY_RIGHT_X, MEETING_DOOR.y),
            Point::new(MEETING_ROOM.x + 24, MEETING_DOOR.y),
            *p,
        ],
        Target::Lab(s) => vec![
            Point::new(HALLWAY_RIGHT_X, LAB_DOOR.y),
            Point::new(LAB_ROOM.x + 24, LAB_DOOR.y),
            Point::new(s.approach_x, s.approach_y),
            Point::new(s.seat_x, s.seat_y),
        ],
        Target::LabQueue(p) => vec![
            Point::new(HALLWAY_RIGHT_X, LAB_DOOR.y),
            Point::new(LAB_ROOM.x + 24, LAB_DOOR.y),
            *p,
        ],
    }
}

/// Door-to-target polyline. The sim is assumed to enter via the west door;
/// this is used for the initial walk-in animation. First element is the
/// inside-of-open-door hallway tile, last is the seated position.
pub fn path_from_door_to(target: &Target) -> Vec<Point> {
    let inside_open_door = Point::new(HALLWAY_LEFT_X, DOOR.y);
    let staging = staging_for_target(target);
    let mut out = vec![inside_open_door];
    out.extend(path_between_hall_nodes(inside_open_door, staging));
    out.extend(target_approach_waypoints(target));
    out
}

/// Prefix returned from `route_from_current_position`. `prefix` is the list of
/// waypoints to walk *before* the hall handoff; `handoff` is where hall
/// transit begins.
struct Prefix {
    prefix: Vec<Point>,
    handoff: Point,
}

/// Given the sim's current (x, y), emit waypoints to the nearest hall node so
/// routing can continue on the corridor grid. Mirrors JS
/// `routeFromCurrentPosition`.
fn route_from_current_position(cx: i32, cy: i32) -> Prefix {
    // Outside (west of the open room): enter via the west door.
    if cx < OPEN_ROOM.x {
        let handoff = Point::new(HALLWAY_LEFT_X, DOOR.y);
        return Prefix {
            prefix: vec![Point::new(OPEN_ROOM.x + 2, DOOR.y), handoff],
            handoff,
        };
    }
    // Inside meeting room: exit via meeting door, then snap to a corridor.
    if (MEETING_ROOM.x..=MEETING_ROOM.x + MEETING_ROOM.w).contains(&cx)
        && (MEETING_ROOM.y..=MEETING_ROOM.y + MEETING_ROOM.h).contains(&cy)
    {
        let corr_y = nearest_corridor_y(MEETING_DOOR.y);
        let handoff = Point::new(HALLWAY_RIGHT_X, corr_y);
        return Prefix {
            prefix: vec![
                Point::new(MEETING_ROOM.x + 24, MEETING_DOOR.y),
                Point::new(HALLWAY_RIGHT_X, MEETING_DOOR.y),
                handoff,
            ],
            handoff,
        };
    }
    // Inside lab room: symmetrical.
    if (LAB_ROOM.x..=LAB_ROOM.x + LAB_ROOM.w).contains(&cx)
        && (LAB_ROOM.y..=LAB_ROOM.y + LAB_ROOM.h).contains(&cy)
    {
        let corr_y = nearest_corridor_y(LAB_DOOR.y);
        let handoff = Point::new(HALLWAY_RIGHT_X, corr_y);
        return Prefix {
            prefix: vec![
                Point::new(LAB_ROOM.x + 24, LAB_DOOR.y),
                Point::new(HALLWAY_RIGHT_X, LAB_DOOR.y),
                handoff,
            ],
            handoff,
        };
    }
    // In the open room: step to the nearest corridor, then to a hall.
    let corr_y = nearest_corridor_y(cy);
    let hall_x = if cx > HALLWAY_RIGHT_X - 20 {
        HALLWAY_RIGHT_X
    } else {
        HALLWAY_LEFT_X
    };
    let mut prefix = Vec::new();
    if (cy - corr_y).abs() > 6 {
        prefix.push(Point::new(cx, corr_y));
    }
    if (cx - hall_x).abs() > 6 {
        prefix.push(Point::new(hall_x, corr_y));
    }
    Prefix {
        prefix,
        handoff: Point::new(hall_x, corr_y),
    }
}

/// Full route from current `(cx, cy)` to `target`. Does NOT include `(cx, cy)`
/// itself — the sim is already there; the route is the list of waypoints to
/// visit in order. Mirrors JS `computeRoute`.
pub fn compute_route(from: Point, target: &Target) -> Vec<Point> {
    let Prefix { prefix, handoff } = route_from_current_position(from.x, from.y);
    let staging = staging_for_target(target);
    let mut out = prefix;
    out.extend(path_between_hall_nodes(handoff, staging));
    out.extend(target_approach_waypoints(target));
    out
}

/// Reverse direction: from a seated target back out through the west door.
/// Mirrors JS `pathToDoorFrom`.
pub fn path_to_door_from(target: &Target) -> Vec<Point> {
    let mut forward = path_from_door_to(target);
    // Drop the last element and reverse.
    forward.pop();
    forward.reverse();
    forward.push(Point::new(OPEN_ROOM.x - 8, DOOR.y));
    forward.push(Point::new(OUTSIDE_X, DOOR.y));
    forward
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::geometry::{
        desk_seats, lab_stations, meeting_seats, LAB_QUEUE_SPOTS, MEETING_QUEUE_SPOTS, QUEUE_SPOTS,
    };
    use proptest::prelude::*;

    #[test]
    fn nearest_corridor_y_snaps() {
        assert_eq!(nearest_corridor_y(125), 125);
        assert_eq!(nearest_corridor_y(256), 256);
        assert_eq!(nearest_corridor_y(180), 125); // |180-125|=55 vs |180-256|=76
        assert_eq!(nearest_corridor_y(200), 256); // |200-125|=75 vs |200-256|=56
        assert_eq!(nearest_corridor_y(-1000), 125);
        assert_eq!(nearest_corridor_y(5000), 576);
    }

    #[test]
    fn on_corridor_membership() {
        assert!(on_corridor(125));
        assert!(on_corridor(256));
        assert!(on_corridor(416));
        assert!(on_corridor(576));
        assert!(!on_corridor(100));
        assert!(!on_corridor(300));
    }

    #[test]
    fn path_between_hall_nodes_same_point_empty() {
        let p = Point::new(HALLWAY_LEFT_X, 256);
        assert_eq!(path_between_hall_nodes(p, p), vec![]);
    }

    #[test]
    fn path_between_hall_nodes_same_column_just_target() {
        let a = Point::new(HALLWAY_LEFT_X, 125);
        let b = Point::new(HALLWAY_LEFT_X, 576);
        assert_eq!(path_between_hall_nodes(a, b), vec![b]);
    }

    #[test]
    fn path_between_hall_nodes_cross_column_snaps() {
        // From = not on a corridor; transit y snaps.
        let a = Point::new(HALLWAY_LEFT_X, 300);
        let b = Point::new(HALLWAY_RIGHT_X, 256);
        // 300 → nearest corridor is 256. Transit y = 256 (= from-snap).
        // Because transit_y (256) == to.y (256), the final push is skipped.
        // Expected: [{HL, 256}, {HR, 256}]
        assert_eq!(
            path_between_hall_nodes(a, b),
            vec![
                Point::new(HALLWAY_LEFT_X, 256),
                Point::new(HALLWAY_RIGHT_X, 256),
            ]
        );
    }

    #[test]
    fn path_between_hall_nodes_cross_column_both_on_corridor() {
        let a = Point::new(HALLWAY_LEFT_X, 125);
        let b = Point::new(HALLWAY_RIGHT_X, 576);
        // transit_y = from.y (on corridor) = 125. Three-step path.
        assert_eq!(
            path_between_hall_nodes(a, b),
            vec![
                Point::new(HALLWAY_RIGHT_X, 125),
                Point::new(HALLWAY_RIGHT_X, 576),
            ]
        );
    }

    #[test]
    fn path_from_door_to_desk_first_row() {
        // First desk in JS iteration: (200, 180). Approach (200, 256),
        // seat (200, 224). Inside open door = (130, 328).
        let desk = desk_seats()[0];
        let t = Target::Desk(desk);
        let r = path_from_door_to(&t);
        // Inside door (130, 328); staging for a desk is (HALLWAY_LEFT_X=130,
        // approach_y=256). Since both endpoints are on HALLWAY_LEFT_X, the
        // hall-node path is [{130, 256}] (same-column case).
        // Then approach waypoints: (200, 256), (200, 224).
        assert_eq!(
            r,
            vec![
                Point::new(130, 328),
                Point::new(130, 256),
                Point::new(200, 256),
                Point::new(200, 224),
            ]
        );
    }

    #[test]
    fn path_from_door_to_meeting_first_seat() {
        // First meeting seat is north side of first cx. TABLE cx = 1004,
        // cy = 207 (MEETING_ROOM.y + h/2 + 14 = 72 + 121 + 14 = 207).
        // TABLE_NORTH_Y = 207 - 30 = 177. First seat cx = 944, seat_y = 155,
        // approach = (944, 127).
        let s = meeting_seats()[0];
        let t = Target::Meeting(s);
        let r = path_from_door_to(&t);
        // Inside open door (130, 328). Staging for meeting:
        // (HALLWAY_RIGHT_X=776, nearest_corridor_y(MEETING_DOOR.y=193))
        // = (776, nearest to 193). Distances to [125,256,416,576]:
        // [68,63,223,383] → 256. Staging = (776, 256).
        // Cross-hall path: transit_y = 328 not in corridors →
        // nearest_corridor_y(328) = 256. Steps:
        // (130,256), (776,256).
        // Approach waypoints: (776, 193), (848, 193), (944, 127), (944, 155).
        assert_eq!(
            r,
            vec![
                Point::new(130, 328),
                Point::new(130, 256),
                Point::new(776, 256),
                Point::new(776, 193),
                Point::new(824 + 24, 193),
                Point::new(944, 127),
                Point::new(944, 155),
            ]
        );
    }

    #[test]
    fn path_from_door_to_lab_first_station() {
        let st = lab_stations()[0];
        let t = Target::Lab(st);
        let r = path_from_door_to(&t);
        // Inside open door (130, 328). Staging for lab:
        // (776, nearest_corridor_y(LAB_DOOR.y=455)). Distances:
        // [330,199,39,121] → 416. Staging = (776, 416).
        // Cross-hall path: transit_y = 328 not corridor → 256. Steps:
        // (130,256), (776,256), (776,416).
        // Approach: (776,455), (848,455), (seat approach), (seat).
        // Station 0: cx = LAB_ROOM.x + LAB_ROOM.w/2 - 110 = 824 + 180 - 110
        //          = 894. seat_y = BENCH.y + BENCH.h + 26 = 362 + 38 + 26 = 426.
        // approach = (894, 458). seat_y = 426.
        assert_eq!(st.seat_x, 894);
        assert_eq!(st.seat_y, 426);
        assert_eq!(st.approach_x, 894);
        assert_eq!(st.approach_y, 458);
        assert_eq!(
            r,
            vec![
                Point::new(130, 328),
                Point::new(130, 256),
                Point::new(776, 256),
                Point::new(776, 416),
                Point::new(776, 455),
                Point::new(848, 455),
                Point::new(894, 458),
                Point::new(894, 426),
            ]
        );
    }

    #[test]
    fn compute_route_outside_start_enters_via_door_then_to_desk() {
        // Simulate a fresh sim spawned just outside the open room.
        let from = Point::new(OUTSIDE_X, DOOR.y);
        let t = Target::Desk(desk_seats()[0]);
        let r = compute_route(from, &t);
        // prefix: [{OPEN_ROOM.x+2=98, 328}, handoff=(130,328)]
        // handoff → staging (130, 256): same-column → [{130,256}]
        // approach: [{200,256},{200,224}]
        assert_eq!(
            r,
            vec![
                Point::new(98, 328),
                Point::new(130, 328),
                Point::new(130, 256),
                Point::new(200, 256),
                Point::new(200, 224),
            ]
        );
    }

    #[test]
    fn compute_route_from_desk_to_meeting() {
        // Sitting at desk (360, 224); move to meeting seat 0.
        let from = Point::new(360, 224);
        let t = Target::Meeting(meeting_seats()[0]);
        let r = compute_route(from, &t);
        // In open room. corr_y = nearest_corridor_y(224). Distances:
        // [99, 32, 192, 352] → 256. hall_x: 360 > 776-20=756? no → HALLWAY_LEFT_X=130.
        // prefix: |224-256|=32>6 → push (360,256); |360-130|=230>6 → push (130,256).
        // handoff = (130, 256). Staging = (776, 256). Hall path cross-column, both
        // on corridor, transit=256: [{776,256}]  (only one push because transit==to.y).
        // Approach for meeting: (776,193), (848,193), (944,127), (944,155).
        assert_eq!(
            r,
            vec![
                Point::new(360, 256),
                Point::new(130, 256),
                Point::new(776, 256),
                Point::new(776, 193),
                Point::new(848, 193),
                Point::new(944, 127),
                Point::new(944, 155),
            ]
        );
    }

    #[test]
    fn compute_route_preserves_continuity() {
        // from → first waypoint should not teleport beyond the next grid step.
        let from = Point::new(360, 224);
        let t = Target::Desk(desk_seats()[5]);
        let r = compute_route(from, &t);
        assert!(!r.is_empty());
        // first step moves on exactly one axis from `from`.
        let first = r[0];
        assert!(
            (first.x == from.x) || (first.y == from.y),
            "first hop {first:?} not axis-aligned with from {from:?}"
        );
    }

    #[test]
    fn path_to_door_from_desk_ends_outside() {
        let t = Target::Desk(desk_seats()[0]);
        let r = path_to_door_from(&t);
        assert_eq!(r.last(), Some(&Point::new(OUTSIDE_X, DOOR.y)));
    }

    /// No two consecutive points are equal.
    fn assert_no_degenerate_steps(route: &[Point]) {
        for w in route.windows(2) {
            assert!(
                w[0] != w[1],
                "degenerate step {:?} == {:?} in route {:?}",
                w[0],
                w[1],
                route
            );
        }
    }

    #[test]
    fn fixed_routes_never_cut_through_desks() {
        // Desk-only targets: the full route must stay axis-aligned, because
        // in the open room there's no "inside-room approach" that can skip
        // a corridor lane.
        let desks = desk_seats();
        let samples: Vec<(Point, Target)> = vec![
            (Point::new(OUTSIDE_X, DOOR.y), Target::Desk(desks[0])),
            (Point::new(OUTSIDE_X, DOOR.y), Target::Desk(desks[11])),
            (Point::new(360, 224), Target::Desk(desks[7])),
        ];
        for (from, t) in samples {
            let r = compute_route(from, &t);
            assert_no_degenerate_steps(&r);
            assert!(!r.is_empty());
            for w in r.windows(2) {
                assert!(
                    w[0].x == w[1].x || w[0].y == w[1].y,
                    "desk route diagonal: {:?} → {:?}",
                    w[0],
                    w[1]
                );
            }
        }
    }

    /// The hall-transit portion of every route is strictly axis-aligned —
    /// meaning all points with `x ∈ {HALLWAY_LEFT_X, HALLWAY_RIGHT_X}` form a
    /// contiguous prefix where each hop is H or V only. Only the in-room
    /// approach tail is allowed to be diagonal (JS Phaser tweens linearly and
    /// the designer accepts that visual shortcut).
    fn assert_hall_transit_axis_aligned(route: &[Point]) {
        // Consecutive pairs where both endpoints sit on a hall column.
        for w in route.windows(2) {
            let a_hall = a_hall_x(w[0].x);
            let b_hall = a_hall_x(w[1].x);
            if a_hall && b_hall {
                assert!(
                    w[0].x == w[1].x || w[0].y == w[1].y,
                    "hall-to-hall diagonal {:?} → {:?}",
                    w[0],
                    w[1]
                );
            }
        }
    }

    fn a_hall_x(x: i32) -> bool {
        x == HALLWAY_LEFT_X || x == HALLWAY_RIGHT_X
    }

    #[test]
    fn meeting_and_lab_routes_no_degenerate_steps() {
        let meets = meeting_seats();
        let labs = lab_stations();
        let samples: Vec<(Point, Target)> = vec![
            (Point::new(OUTSIDE_X, DOOR.y), Target::Meeting(meets[0])),
            (Point::new(OUTSIDE_X, DOOR.y), Target::Lab(labs[2])),
            (Point::new(360, 224), Target::Meeting(meets[3])),
            (Point::new(680, 544), Target::Lab(labs[0])),
        ];
        for (from, t) in samples {
            let r = compute_route(from, &t);
            assert_no_degenerate_steps(&r);
            assert_hall_transit_axis_aligned(&r);
            assert!(!r.is_empty());
        }
    }

    // Property tests — random starts × random targets stay on the grid.

    fn arbitrary_target() -> impl Strategy<Value = Target> {
        let desks = desk_seats();
        let meets = meeting_seats();
        let labs = lab_stations();
        prop_oneof![
            (0usize..desks.len()).prop_map(move |i| Target::Desk(desks[i])),
            (0usize..meets.len()).prop_map(move |i| Target::Meeting(meets[i])),
            (0usize..labs.len()).prop_map(move |i| Target::Lab(labs[i])),
            (0usize..QUEUE_SPOTS.len()).prop_map(|i| Target::Queue(QUEUE_SPOTS[i])),
            (0usize..MEETING_QUEUE_SPOTS.len())
                .prop_map(|i| Target::MeetingQueue(MEETING_QUEUE_SPOTS[i])),
            (0usize..LAB_QUEUE_SPOTS.len()).prop_map(|i| Target::LabQueue(LAB_QUEUE_SPOTS[i])),
        ]
    }

    proptest! {
        #[test]
        fn hall_transit_axis_aligned(
            fx in 0i32..1280,
            fy in 0i32..640,
            target in arbitrary_target(),
        ) {
            // Diagonal hops are only permitted when at least one endpoint is
            // off the hall grid (inside-room approach tail). Hall-to-hall
            // transit must always turn on cardinal axes.
            let r = compute_route(Point::new(fx, fy), &target);
            for w in r.windows(2) {
                let a_hall = a_hall_x(w[0].x);
                let b_hall = a_hall_x(w[1].x);
                if a_hall && b_hall {
                    prop_assert!(w[0].x == w[1].x || w[0].y == w[1].y);
                }
            }
        }

        #[test]
        fn compute_route_no_degenerate_steps(
            fx in 0i32..1280,
            fy in 0i32..640,
            target in arbitrary_target(),
        ) {
            let r = compute_route(Point::new(fx, fy), &target);
            for w in r.windows(2) {
                prop_assert!(w[0] != w[1]);
            }
        }

        /// Horizontal hops inside the hall-transit core (points with x ∈ {HL, HR}
        /// on *both* endpoints, or y equal on both endpoints) must be at a
        /// corridor-y. We only check horizontal hops that span the full hall
        /// width — those are the "cross-hall transit" legs and must be safe.
        #[test]
        fn cross_hall_horizontal_on_corridor(
            fx in 0i32..1280,
            fy in 0i32..640,
            target in arbitrary_target(),
        ) {
            let r = compute_route(Point::new(fx, fy), &target);
            for w in r.windows(2) {
                let (a, b) = (w[0], w[1]);
                let is_hl_hr_horizontal = (a.x == HALLWAY_LEFT_X && b.x == HALLWAY_RIGHT_X)
                    || (a.x == HALLWAY_RIGHT_X && b.x == HALLWAY_LEFT_X);
                if is_hl_hr_horizontal && a.y == b.y {
                    prop_assert!(on_corridor(a.y),
                        "horizontal hall-to-hall hop at y={} off corridor", a.y);
                }
            }
        }

        #[test]
        fn path_between_hall_nodes_no_degenerate_steps(
            fy in 0i32..640,
            ty in 0i32..640,
            from_left in any::<bool>(),
            to_left in any::<bool>(),
        ) {
            let fx = if from_left { HALLWAY_LEFT_X } else { HALLWAY_RIGHT_X };
            let tx = if to_left { HALLWAY_LEFT_X } else { HALLWAY_RIGHT_X };
            let from = Point::new(fx, fy);
            let to = Point::new(tx, ty);
            let r = path_between_hall_nodes(from, to);
            // Build the full walk including `from` as the implicit first node.
            let mut full = vec![from];
            full.extend(r);
            for w in full.windows(2) {
                prop_assert!(w[0] != w[1]);
            }
            prop_assert_eq!(full.last().copied(), Some(to));
        }
    }
}
