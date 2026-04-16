//! Pure layout / routing / classification logic. No rendering, no I/O. These
//! modules mirror `public/main.js` 1:1 so a future Rust renderer can diff
//! pixel-for-pixel against the browser Phaser build.

pub mod classify;
pub mod geometry;
pub mod palette;
pub mod routing;

pub use classify::{classify, Room, LAB_KEYWORDS};
pub use geometry::{
    desk_seats, lab_stations, meeting_seats, DeskSeat, DoorV, LabStation, MeetingSeat,
    MeetingSide, Point, Rect, Spill, Table, WindowRec, APPROACH_OFFSET_Y, BENCH, CORRIDOR_YS,
    DESK_COLS, DESK_H, DESK_ROWS, DESK_W, DOOR, HALLWAY_LEFT_X, HALLWAY_RIGHT_X, LAB_DOOR,
    LAB_QUEUE_SPOTS, LAB_ROOM, LAB_STATION_XS, MEETING_DOOR, MEETING_QUEUE_SPOTS, MEETING_ROOM,
    NORTH_CORRIDOR_Y, OPEN_ROOM, OUTSIDE_X, QUEUE_SPOTS, SEAT_OFFSET_Y, TABLE, TABLE_NORTH_Y,
    TABLE_SOUTH_Y, WALL_THICKNESS, WINDOWS, WINDOW_H, WINDOW_W, WORLD_H, WORLD_W,
};
pub use palette::{
    hash_str, mote_color, sim_colors, Rgb, SimColors, MOTE_COLORS, MOTE_DEFAULT_COLOR, SHIRT_HUES,
    SKIN_TONES,
};
pub use routing::{
    compute_route, nearest_corridor_y, on_corridor, path_between_hall_nodes, path_from_door_to,
    path_to_door_from, staging_for_target, target_approach_waypoints, Target,
};
