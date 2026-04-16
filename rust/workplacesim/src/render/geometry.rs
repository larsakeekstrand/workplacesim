//! Layout constants + simple geometric types. 1:1 port of the top-of-file
//! constants in `public/main.js`; names are snake_case but values match.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub const fn right(&self) -> i32 {
        self.x + self.w
    }
    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub const fn center(&self) -> Point {
        Point::new(self.x + self.w / 2, self.y + self.h / 2)
    }
    pub const fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
}

pub const WORLD_W: i32 = 1280;
pub const WORLD_H: i32 = 640;

pub const OPEN_ROOM: Rect = Rect::new(96, 72, 704, 512);

// Right column is split: meeting on top, lab below.
pub const RIGHT_COL_X: i32 = OPEN_ROOM.x + OPEN_ROOM.w + 24; // 824
pub const RIGHT_COL_W: i32 = 360;
pub const RIGHT_COL_GAP_Y: i32 = 320;
pub const RIGHT_COL_WALL: i32 = 12;

pub const MEETING_ROOM: Rect = Rect::new(
    RIGHT_COL_X,
    OPEN_ROOM.y,
    RIGHT_COL_W,
    RIGHT_COL_GAP_Y - OPEN_ROOM.y - RIGHT_COL_WALL / 2,
);

pub const LAB_ROOM: Rect = Rect::new(
    RIGHT_COL_X,
    RIGHT_COL_GAP_Y + RIGHT_COL_WALL / 2,
    RIGHT_COL_W,
    OPEN_ROOM.y + OPEN_ROOM.h - (RIGHT_COL_GAP_Y + RIGHT_COL_WALL / 2),
);

// DOOR is a full rect (has w); meeting/lab doors only carry {x, y, h} in JS.
pub const DOOR: Rect = Rect::new(OPEN_ROOM.x, OPEN_ROOM.y + OPEN_ROOM.h / 2, 10, 64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorV {
    pub x: i32,
    pub y: i32,
    pub h: i32,
}

pub const MEETING_DOOR: DoorV = DoorV {
    x: OPEN_ROOM.x + OPEN_ROOM.w,
    y: MEETING_ROOM.y + MEETING_ROOM.h / 2,
    h: 60,
};

pub const LAB_DOOR: DoorV = DoorV {
    x: OPEN_ROOM.x + OPEN_ROOM.w,
    y: LAB_ROOM.y + LAB_ROOM.h / 2,
    h: 60,
};

pub const OUTSIDE_X: i32 = 40;

pub const DESK_COLS: [i32; 4] = [200, 360, 520, 680];
pub const DESK_ROWS: [i32; 3] = [180, 340, 500];
pub const DESK_W: i32 = 96;
pub const DESK_H: i32 = 46;

pub const SEAT_OFFSET_Y: i32 = 44;
pub const APPROACH_OFFSET_Y: i32 = 76;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Table {
    pub cx: i32,
    pub cy: i32,
    pub w: i32,
    pub h: i32,
}

pub const TABLE: Table = Table {
    cx: MEETING_ROOM.x + MEETING_ROOM.w / 2,
    cy: MEETING_ROOM.y + MEETING_ROOM.h / 2 + 14,
    w: 220,
    h: 60,
};

pub const BENCH: Rect = Rect::new(LAB_ROOM.x + 24, LAB_ROOM.y + 36, LAB_ROOM.w - 48, 38);

pub const LAB_STATION_XS: [i32; 3] = [
    LAB_ROOM.x + LAB_ROOM.w / 2 - 110,
    LAB_ROOM.x + LAB_ROOM.w / 2,
    LAB_ROOM.x + LAB_ROOM.w / 2 + 110,
];

// Corridor grid. Halls are just outside the desk x-range; corridor-ys follow
// the desk-approach waypoints so the horizontal leg is always desk-safe.
pub const HALLWAY_LEFT_X: i32 = OPEN_ROOM.x + 34; // 130
pub const HALLWAY_RIGHT_X: i32 = OPEN_ROOM.x + OPEN_ROOM.w - 24; // 776
pub const NORTH_CORRIDOR_Y: i32 = OPEN_ROOM.y + 53; // 125

pub const CORRIDOR_YS: [i32; 4] = [
    NORTH_CORRIDOR_Y,
    DESK_ROWS[0] + APPROACH_OFFSET_Y,
    DESK_ROWS[1] + APPROACH_OFFSET_Y,
    DESK_ROWS[2] + APPROACH_OFFSET_Y,
];

pub const WINDOW_W: i32 = 40;
pub const WINDOW_H: i32 = 6;
pub const WALL_THICKNESS: i32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spill {
    North,
    South,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRec {
    pub x: i32,
    pub y: i32,
    pub spill: Spill,
    pub room_edge_y: i32,
}

const fn north_wall_y(room: Rect) -> i32 {
    room.y - WALL_THICKNESS / 2
}
const fn south_wall_y(room: Rect) -> i32 {
    room.y + room.h + WALL_THICKNESS / 2
}

pub const WINDOWS: [WindowRec; 9] = [
    // OPEN_ROOM north wall
    WindowRec {
        x: 240,
        y: north_wall_y(OPEN_ROOM),
        spill: Spill::South,
        room_edge_y: OPEN_ROOM.y,
    },
    WindowRec {
        x: 448,
        y: north_wall_y(OPEN_ROOM),
        spill: Spill::South,
        room_edge_y: OPEN_ROOM.y,
    },
    WindowRec {
        x: 656,
        y: north_wall_y(OPEN_ROOM),
        spill: Spill::South,
        room_edge_y: OPEN_ROOM.y,
    },
    // OPEN_ROOM south wall
    WindowRec {
        x: 320,
        y: south_wall_y(OPEN_ROOM),
        spill: Spill::North,
        room_edge_y: OPEN_ROOM.y + OPEN_ROOM.h,
    },
    WindowRec {
        x: 576,
        y: south_wall_y(OPEN_ROOM),
        spill: Spill::North,
        room_edge_y: OPEN_ROOM.y + OPEN_ROOM.h,
    },
    // MEETING_ROOM north wall
    WindowRec {
        x: MEETING_ROOM.x + 40,
        y: north_wall_y(MEETING_ROOM),
        spill: Spill::South,
        room_edge_y: MEETING_ROOM.y,
    },
    WindowRec {
        x: MEETING_ROOM.x + MEETING_ROOM.w - 40,
        y: north_wall_y(MEETING_ROOM),
        spill: Spill::South,
        room_edge_y: MEETING_ROOM.y,
    },
    // LAB_ROOM south wall
    WindowRec {
        x: LAB_ROOM.x + LAB_ROOM.w / 2 - 60,
        y: south_wall_y(LAB_ROOM),
        spill: Spill::North,
        room_edge_y: LAB_ROOM.y + LAB_ROOM.h,
    },
    WindowRec {
        x: LAB_ROOM.x + LAB_ROOM.w / 2 + 60,
        y: south_wall_y(LAB_ROOM),
        spill: Spill::North,
        room_edge_y: LAB_ROOM.y + LAB_ROOM.h,
    },
];

pub const QUEUE_SPOTS: [Point; 4] = [
    Point::new(OPEN_ROOM.x + OPEN_ROOM.w - 48, OPEN_ROOM.y + 48),
    Point::new(OPEN_ROOM.x + OPEN_ROOM.w - 48, OPEN_ROOM.y + 108),
    Point::new(OPEN_ROOM.x + OPEN_ROOM.w - 48, OPEN_ROOM.y + OPEN_ROOM.h - 100),
    Point::new(OPEN_ROOM.x + OPEN_ROOM.w - 48, OPEN_ROOM.y + OPEN_ROOM.h - 40),
];

pub const MEETING_QUEUE_SPOTS: [Point; 2] = [
    Point::new(MEETING_ROOM.x + 32, MEETING_ROOM.y + 60),
    Point::new(MEETING_ROOM.x + MEETING_ROOM.w - 32, MEETING_ROOM.y + 60),
];

pub const LAB_QUEUE_SPOTS: [Point; 2] = [
    Point::new(LAB_ROOM.x + 32, LAB_ROOM.y + LAB_ROOM.h - 28),
    Point::new(LAB_ROOM.x + LAB_ROOM.w - 32, LAB_ROOM.y + LAB_ROOM.h - 28),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeskSeat {
    pub x: i32,
    pub y: i32,
    pub seat_x: i32,
    pub seat_y: i32,
    pub approach_x: i32,
    pub approach_y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeetingSide {
    North,
    South,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeetingSeat {
    pub x: i32,
    pub y: i32,
    pub seat_x: i32,
    pub seat_y: i32,
    pub approach_x: i32,
    pub approach_y: i32,
    pub side: MeetingSide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LabStation {
    pub x: i32,
    pub y: i32,
    pub seat_x: i32,
    pub seat_y: i32,
    pub approach_x: i32,
    pub approach_y: i32,
}

/// 12 desk seats in `DESK_ROWS × DESK_COLS` (row-major, same iteration as JS).
pub fn desk_seats() -> Vec<DeskSeat> {
    let mut out = Vec::with_capacity(DESK_ROWS.len() * DESK_COLS.len());
    for &y in &DESK_ROWS {
        for &x in &DESK_COLS {
            out.push(DeskSeat {
                x,
                y,
                seat_x: x,
                seat_y: y + SEAT_OFFSET_Y,
                approach_x: x,
                approach_y: y + APPROACH_OFFSET_Y,
            });
        }
    }
    out
}

pub const TABLE_NORTH_Y: i32 = TABLE.cy - TABLE.h / 2;
pub const TABLE_SOUTH_Y: i32 = TABLE.cy + TABLE.h / 2;

/// 4 meeting seats — two per side of the table.
pub fn meeting_seats() -> Vec<MeetingSeat> {
    let mut out = Vec::with_capacity(4);
    for &cx in &[TABLE.cx - 60, TABLE.cx + 60] {
        out.push(MeetingSeat {
            x: cx,
            y: TABLE_NORTH_Y - 14,
            seat_x: cx,
            seat_y: TABLE_NORTH_Y - 22,
            approach_x: cx,
            approach_y: TABLE_NORTH_Y - 50,
            side: MeetingSide::North,
        });
        out.push(MeetingSeat {
            x: cx,
            y: TABLE_SOUTH_Y + 14,
            seat_x: cx,
            seat_y: TABLE_SOUTH_Y + 22,
            approach_x: cx,
            approach_y: TABLE_SOUTH_Y + 50,
            side: MeetingSide::South,
        });
    }
    out
}

/// 3 lab stations along the bench.
pub fn lab_stations() -> Vec<LabStation> {
    let seat_y = BENCH.y + BENCH.h + 26;
    LAB_STATION_XS
        .iter()
        .map(|&cx| LabStation {
            x: cx,
            y: seat_y - 8,
            seat_x: cx,
            seat_y,
            approach_x: cx,
            approach_y: seat_y + 32,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_dimensions() {
        assert_eq!(WORLD_W, 1280);
        assert_eq!(WORLD_H, 640);
    }

    #[test]
    fn approach_offset_matches_literal() {
        assert_eq!(APPROACH_OFFSET_Y, 76);
    }

    #[test]
    fn hallway_xs_match_literal() {
        assert_eq!(HALLWAY_LEFT_X, 130);
        assert_eq!(HALLWAY_RIGHT_X, 776);
    }

    #[test]
    fn corridor_ys_match_literal() {
        assert_eq!(CORRIDOR_YS, [125, 256, 416, 576]);
    }

    #[test]
    fn corridor_ys_strictly_ascending() {
        for w in CORRIDOR_YS.windows(2) {
            assert!(w[0] < w[1], "{} !< {}", w[0], w[1]);
        }
    }

    #[test]
    fn corridor_ys_follow_desk_approach() {
        for (i, &row_y) in DESK_ROWS.iter().enumerate() {
            assert_eq!(CORRIDOR_YS[i + 1], row_y + APPROACH_OFFSET_Y);
        }
    }

    #[test]
    fn hallway_topology() {
        // Both halls are interior to the open room — the desks are between them.
        // These are constants, so we validate via const-checks instead of runtime
        // assertions (which clippy optimizes to `assert!(true)`).
        const _: () = assert!(OPEN_ROOM.x < HALLWAY_LEFT_X);
        const _: () = assert!(HALLWAY_LEFT_X < HALLWAY_RIGHT_X);
        const _: () = assert!(HALLWAY_RIGHT_X < OPEN_ROOM.right());
        // Desk columns sit between the two halls.
        for &c in &DESK_COLS {
            assert!(HALLWAY_LEFT_X < c, "desk col {c} not east of HALLWAY_LEFT_X");
            assert!(c < HALLWAY_RIGHT_X, "desk col {c} not west of HALLWAY_RIGHT_X");
        }
    }

    #[test]
    fn desk_count_twelve() {
        assert_eq!(DESK_COLS.len() * DESK_ROWS.len(), 12);
        assert_eq!(desk_seats().len(), 12);
    }

    #[test]
    fn meeting_seat_count_four() {
        assert_eq!(meeting_seats().len(), 4);
    }

    #[test]
    fn lab_station_count_three() {
        assert_eq!(lab_stations().len(), 3);
    }

    #[test]
    fn every_desk_approach_on_corridor_y() {
        for d in desk_seats() {
            assert!(
                CORRIDOR_YS.contains(&d.approach_y),
                "desk approach {} missing from CORRIDOR_YS",
                d.approach_y
            );
        }
    }

    #[test]
    fn rect_center_and_contains() {
        let r = Rect::new(10, 20, 40, 60);
        assert_eq!(r.right(), 50);
        assert_eq!(r.bottom(), 80);
        assert_eq!(r.center(), Point::new(30, 50));
        assert!(r.contains(Point::new(10, 20)));
        assert!(r.contains(Point::new(50, 80)));
        assert!(!r.contains(Point::new(9, 20)));
        assert!(!r.contains(Point::new(10, 81)));
    }

    #[test]
    fn meeting_room_bounds() {
        assert_eq!(MEETING_ROOM.x, 824);
        assert_eq!(MEETING_ROOM.y, 72);
        assert_eq!(MEETING_ROOM.w, 360);
        assert_eq!(MEETING_ROOM.h, 242);
    }

    #[test]
    fn lab_room_bounds() {
        assert_eq!(LAB_ROOM.x, 824);
        assert_eq!(LAB_ROOM.y, 326);
        assert_eq!(LAB_ROOM.w, 360);
        assert_eq!(LAB_ROOM.h, 258);
    }

    #[test]
    fn doors_match_js() {
        assert_eq!(DOOR.x, 96);
        assert_eq!(DOOR.y, 328);
        assert_eq!(MEETING_DOOR.x, 800);
        assert_eq!(MEETING_DOOR.y, 193);
        assert_eq!(LAB_DOOR.x, 800);
        assert_eq!(LAB_DOOR.y, 455);
    }

    #[test]
    fn windows_have_nine_entries() {
        assert_eq!(WINDOWS.len(), 9);
    }
}
