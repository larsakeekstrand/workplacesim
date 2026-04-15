/* global Phaser */

const WORLD_W = 1280;
const WORLD_H = 640;

const OPEN_ROOM = { x: 96, y: 72, w: 704, h: 512 };

// right column is split: meeting upper, lab lower
const RIGHT_COL_X = OPEN_ROOM.x + OPEN_ROOM.w + 24;
const RIGHT_COL_W = 360;
const RIGHT_COL_GAP_Y = 320;
const RIGHT_COL_WALL = 12;

const MEETING_ROOM = {
  x: RIGHT_COL_X,
  y: OPEN_ROOM.y,
  w: RIGHT_COL_W,
  h: RIGHT_COL_GAP_Y - OPEN_ROOM.y - RIGHT_COL_WALL / 2,
};
const LAB_ROOM = {
  x: RIGHT_COL_X,
  y: RIGHT_COL_GAP_Y + RIGHT_COL_WALL / 2,
  w: RIGHT_COL_W,
  h: OPEN_ROOM.y + OPEN_ROOM.h - (RIGHT_COL_GAP_Y + RIGHT_COL_WALL / 2),
};

const DOOR = { x: OPEN_ROOM.x, y: OPEN_ROOM.y + OPEN_ROOM.h / 2, w: 10, h: 64 };
const MEETING_DOOR = {
  x: OPEN_ROOM.x + OPEN_ROOM.w,
  y: MEETING_ROOM.y + MEETING_ROOM.h / 2,
  h: 60,
};
const LAB_DOOR = {
  x: OPEN_ROOM.x + OPEN_ROOM.w,
  y: LAB_ROOM.y + LAB_ROOM.h / 2,
  h: 60,
};
const OUTSIDE_X = 40;

const DESK_COLS = [200, 360, 520, 680];
const DESK_ROWS = [180, 340, 500];
const DESK_W = 96;
const DESK_H = 46;

const SEAT_OFFSET_Y = 44;
const APPROACH_OFFSET_Y = 76;

const TABLE = {
  cx: MEETING_ROOM.x + MEETING_ROOM.w / 2,
  cy: MEETING_ROOM.y + MEETING_ROOM.h / 2 + 14,
  w: 220,
  h: 60,
};

const BENCH = {
  x: LAB_ROOM.x + 24,
  y: LAB_ROOM.y + 36,
  w: LAB_ROOM.w - 48,
  h: 38,
};
const LAB_STATION_XS = [
  LAB_ROOM.x + LAB_ROOM.w / 2 - 110,
  LAB_ROOM.x + LAB_ROOM.w / 2,
  LAB_ROOM.x + LAB_ROOM.w / 2 + 110,
];

const PALETTE = {
  floorA: 0x3a2f24,
  floorB: 0x433627,
  floorMeetingA: 0x2c2f3a,
  floorMeetingB: 0x353948,
  floorLabA: 0x1f3038,
  floorLabB: 0x263a44,
  floorLine: 0x2a2218,
  wall: 0x1a2028,
  wallHi: 0x2d3643,
  deskTop: 0x8a6a3f,
  deskEdge: 0x5c4526,
  deskShade: 0x6e5432,
  monitor: 0x0f1722,
  monitorGlow: 0x6ec6ff,
  keyboard: 0x1c232d,
  mouse: 0x2a2f39,
  chair: 0x2b333d,
  chairHi: 0x3e4856,
  plant: 0x4aa35a,
  plantPot: 0x6a4026,
  lampGlow: 0xffd27a,
  whiteboardFrame: 0xa0a8b4,
  whiteboardBody: 0xf2f2ee,
  windowFrame: 0x3d4956,
  windowGlass: 0x7ab5d6,
  benchTop: 0xc8cdd5,
  benchShade: 0x6f7682,
  benchEdge: 0x4e535c,
  scope: 0x0a131a,
  scopeTrace: 0x5cffaf,
  led: 0x66ff88,
  ledOff: 0x223028,
  shadow: 0x000000,
};

const SKIN_TONES = [0xf5cfa6, 0xe3b58a, 0xc48f6c, 0x8d5a3d, 0xf0d4b4];
const SHIRT_HUES = [210, 340, 40, 140, 260, 20, 190, 300, 80, 170];

function hashStr(s) {
  let h = 0;
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return h;
}

function truncate(s, n) {
  if (!s) return "";
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

// Render a path as "parent/basename" — one directory level of context, no more.
function shortPath(p) {
  if (!p) return "";
  const clean = String(p).replace(/\\/g, "/").replace(/\/+$/, "");
  const parts = clean.split("/").filter(Boolean);
  if (!parts.length) return "";
  if (parts.length === 1) return parts[0];
  return parts.slice(-2).join("/");
}

function shirtColor(user) {
  const h = hashStr(user || "?");
  const hue = SHIRT_HUES[h % SHIRT_HUES.length];
  return Phaser.Display.Color.HSLToColor(hue / 360, 0.55, 0.52).color;
}

function pantsColor(user) {
  const h = hashStr(user || "?") >> 3;
  const hue = SHIRT_HUES[(h + 3) % SHIRT_HUES.length];
  return Phaser.Display.Color.HSLToColor(hue / 360, 0.35, 0.28).color;
}

function skinColor(user) {
  return SKIN_TONES[hashStr(user || "?") % SKIN_TONES.length];
}

const FOOTSTEP_LIFETIME_MS = 900;
const FOOTSTEP_INTERVAL_MS = 120;
const FOOTSTEP_MAX = 6;
const TETHER_LIFETIME_MS = 2000;
const GLYPH_IDLE_MS = 60_000;
const WINDOW_SPILL_BASE_ALPHA = 0.08;
const WINDOW_SPILL_PEAK_ALPHA = 0.22;
const WINDOW_EVENT_WINDOW_MS = 10_000;
const MOTE_LIFETIME_MS = 1200;
const MOTE_CAP = 40;
// rough pixel width per monospace char at the ticker font size
const AVG_CHAR_WIDTH_PX = 5.2;
const MOTE_COLORS = {
  Read: 0x7fc7ff,
  Grep: 0x7fc7ff,
  Glob: 0x7fc7ff,
  LS: 0x7fc7ff,
  NotebookRead: 0x7fc7ff,
  Write: 0xffb86c,
  Edit: 0xffb86c,
  MultiEdit: 0xffb86c,
  NotebookEdit: 0xffb86c,
  Bash: 0x8be98b,
  Agent: 0xff8fd4,
  Task: 0xff8fd4,
  TaskCreate: 0xff8fd4,
  WebFetch: 0xc28fff,
  WebSearch: 0xc28fff,
};
const MOTE_DEFAULT_COLOR = 0xcccccc;

const LAB_KEYWORDS = [
  "test",
  "spec",
  "review",
  "verify",
  "verifier",
  "lint",
  "bench",
  "analyzer",
  "hunter",
  "qa",
];
function isLabAgent(agent) {
  const haystack = `${agent.agent_type || ""} ${agent.description || ""}`.toLowerCase();
  return LAB_KEYWORDS.some((k) => haystack.includes(k));
}

const desks = [];
for (const y of DESK_ROWS) {
  for (const x of DESK_COLS) {
    desks.push({
      x,
      y,
      seatX: x,
      seatY: y + SEAT_OFFSET_Y,
      approachX: x,
      approachY: y + APPROACH_OFFSET_Y,
      taken: false,
    });
  }
}

const TABLE_NORTH_Y = TABLE.cy - TABLE.h / 2;
const TABLE_SOUTH_Y = TABLE.cy + TABLE.h / 2;
const meetingSeats = [];
for (const cx of [TABLE.cx - 60, TABLE.cx + 60]) {
  meetingSeats.push({
    x: cx,
    y: TABLE_NORTH_Y - 14,
    seatX: cx,
    seatY: TABLE_NORTH_Y - 22,
    approachX: cx,
    approachY: TABLE_NORTH_Y - 50,
    side: "north",
    taken: false,
  });
  meetingSeats.push({
    x: cx,
    y: TABLE_SOUTH_Y + 14,
    seatX: cx,
    seatY: TABLE_SOUTH_Y + 22,
    approachX: cx,
    approachY: TABLE_SOUTH_Y + 50,
    side: "south",
    taken: false,
  });
}

const labStations = LAB_STATION_XS.map((cx) => {
  const seatY = BENCH.y + BENCH.h + 26;
  return {
    x: cx,
    y: seatY - 8,
    seatX: cx,
    seatY,
    approachX: cx,
    approachY: seatY + 32,
    taken: false,
  };
});

const WINDOW_W = 40;
const WINDOW_H = 6;
const WALL_THICKNESS = 6;
const northWallY = (room) => room.y - WALL_THICKNESS / 2;
const southWallY = (room) => room.y + room.h + WALL_THICKNESS / 2;
const WINDOWS = [
  // OPEN_ROOM north wall
  { x: 240, y: northWallY(OPEN_ROOM), spill: "south", roomEdgeY: OPEN_ROOM.y },
  { x: 448, y: northWallY(OPEN_ROOM), spill: "south", roomEdgeY: OPEN_ROOM.y },
  { x: 656, y: northWallY(OPEN_ROOM), spill: "south", roomEdgeY: OPEN_ROOM.y },
  // OPEN_ROOM south wall
  { x: 320, y: southWallY(OPEN_ROOM), spill: "north", roomEdgeY: OPEN_ROOM.y + OPEN_ROOM.h },
  { x: 576, y: southWallY(OPEN_ROOM), spill: "north", roomEdgeY: OPEN_ROOM.y + OPEN_ROOM.h },
  // MEETING_ROOM north wall (flanking the whiteboard)
  { x: MEETING_ROOM.x + 40, y: northWallY(MEETING_ROOM), spill: "south", roomEdgeY: MEETING_ROOM.y },
  { x: MEETING_ROOM.x + MEETING_ROOM.w - 40, y: northWallY(MEETING_ROOM), spill: "south", roomEdgeY: MEETING_ROOM.y },
  // LAB_ROOM south wall
  { x: LAB_ROOM.x + LAB_ROOM.w / 2 - 60, y: southWallY(LAB_ROOM), spill: "north", roomEdgeY: LAB_ROOM.y + LAB_ROOM.h },
  { x: LAB_ROOM.x + LAB_ROOM.w / 2 + 60, y: southWallY(LAB_ROOM), spill: "north", roomEdgeY: LAB_ROOM.y + LAB_ROOM.h },
];

const QUEUE_SPOTS = [
  { x: OPEN_ROOM.x + OPEN_ROOM.w - 48, y: OPEN_ROOM.y + 48 },
  { x: OPEN_ROOM.x + OPEN_ROOM.w - 48, y: OPEN_ROOM.y + 108 },
  { x: OPEN_ROOM.x + OPEN_ROOM.w - 48, y: OPEN_ROOM.y + OPEN_ROOM.h - 100 },
  { x: OPEN_ROOM.x + OPEN_ROOM.w - 48, y: OPEN_ROOM.y + OPEN_ROOM.h - 40 },
];

const MEETING_QUEUE_SPOTS = [
  { x: MEETING_ROOM.x + 32, y: MEETING_ROOM.y + 60 },
  { x: MEETING_ROOM.x + MEETING_ROOM.w - 32, y: MEETING_ROOM.y + 60 },
];

const LAB_QUEUE_SPOTS = [
  { x: LAB_ROOM.x + 32, y: LAB_ROOM.y + LAB_ROOM.h - 28 },
  { x: LAB_ROOM.x + LAB_ROOM.w - 32, y: LAB_ROOM.y + LAB_ROOM.h - 28 },
];

// Corridor routing grid. Straight-line tweens cut through desks; sims must
// hug these lanes instead. Constants are derived from the desk layout:
// halls are just outside the desk x-range (cols span 152–728) and corridors
// are just outside each row's y-range (rows at 180, 340, 500).
const HALLWAY_LEFT_X = OPEN_ROOM.x + 34;
const HALLWAY_RIGHT_X = OPEN_ROOM.x + OPEN_ROOM.w - 24;
const NORTH_CORRIDOR_Y = OPEN_ROOM.y + 53;
const CORRIDOR_YS = [
  NORTH_CORRIDOR_Y,
  DESK_ROWS[0] + APPROACH_OFFSET_Y,
  DESK_ROWS[1] + APPROACH_OFFSET_Y,
  DESK_ROWS[2] + APPROACH_OFFSET_Y,
];

function nearestCorridorY(y) {
  let best = CORRIDOR_YS[0];
  let bestD = Math.abs(y - best);
  for (const cy of CORRIDOR_YS) {
    const d = Math.abs(y - cy);
    if (d < bestD) {
      bestD = d;
      best = cy;
    }
  }
  return best;
}

function stagingForTarget(target) {
  if (target.kind === "desk") return { x: HALLWAY_LEFT_X, y: target.desk.approachY };
  if (target.kind === "queue") return { x: HALLWAY_RIGHT_X, y: nearestCorridorY(target.y) };
  if (target.kind === "meeting" || target.kind === "meeting-queue") {
    return { x: HALLWAY_RIGHT_X, y: nearestCorridorY(MEETING_DOOR.y) };
  }
  // lab / lab-queue
  return { x: HALLWAY_RIGHT_X, y: nearestCorridorY(LAB_DOOR.y) };
}

function pathBetweenHallNodes(from, to) {
  if (from.x === to.x && from.y === to.y) return [];
  if (from.x === to.x) return [to];
  // Cross-hall: the horizontal leg must be at a corridor y (not e.g. DOOR.y,
  // which cuts through row 2). Snap first if needed.
  const transitY = CORRIDOR_YS.includes(from.y) ? from.y : nearestCorridorY(from.y);
  const out = [];
  if (transitY !== from.y) out.push({ x: from.x, y: transitY });
  if (transitY !== to.y || from.x !== to.x) out.push({ x: to.x, y: transitY });
  if (transitY !== to.y) out.push(to);
  return out;
}

function targetApproachWaypoints(target) {
  if (target.kind === "desk") {
    return [
      { x: target.desk.approachX, y: target.desk.approachY },
      { x: target.desk.seatX, y: target.desk.seatY },
    ];
  }
  if (target.kind === "queue") return [{ x: target.x, y: target.y }];
  if (target.kind === "meeting") {
    return [
      { x: HALLWAY_RIGHT_X, y: MEETING_DOOR.y },
      { x: MEETING_ROOM.x + 24, y: MEETING_DOOR.y },
      { x: target.seat.approachX, y: target.seat.approachY },
      { x: target.seat.seatX, y: target.seat.seatY },
    ];
  }
  if (target.kind === "meeting-queue") {
    return [
      { x: HALLWAY_RIGHT_X, y: MEETING_DOOR.y },
      { x: MEETING_ROOM.x + 24, y: MEETING_DOOR.y },
      { x: target.x, y: target.y },
    ];
  }
  if (target.kind === "lab") {
    return [
      { x: HALLWAY_RIGHT_X, y: LAB_DOOR.y },
      { x: LAB_ROOM.x + 24, y: LAB_DOOR.y },
      { x: target.station.approachX, y: target.station.approachY },
      { x: target.station.seatX, y: target.station.seatY },
    ];
  }
  // lab-queue
  return [
    { x: HALLWAY_RIGHT_X, y: LAB_DOOR.y },
    { x: LAB_ROOM.x + 24, y: LAB_DOOR.y },
    { x: target.x, y: target.y },
  ];
}

function findFreeDesk() {
  return desks.find((d) => !d.taken) || null;
}
function findFreeMeetingSeat() {
  return meetingSeats.find((s) => !s.taken) || null;
}
function findFreeLabStation() {
  return labStations.find((s) => !s.taken) || null;
}

class RoomScene extends Phaser.Scene {
  constructor() {
    super("room");
    this.sims = new Map();
    this.queuedOverflow = 0;
    this.queuedMeetingOverflow = 0;
    this.queuedLabOverflow = 0;
    this.tethers = [];
    this.motes = [];
    this.recentEvents = [];
    this.windowSpills = [];
    this.fileTicker = [];
    this.fileTickerDirty = false;
    this.labMonitorCursor = 0;
  }

  create() {
    this.drawFloor();
    this.drawWalls();
    this.drawWindows();
    this.drawDecor();
    for (const d of desks) this.drawDesk(d);
    this.drawMeetingRoom();
    this.drawLabRoom();
    this.drawFileTicker();
    this.effects = this.add.graphics();
    this.buildSimTextures();
    this.connect();
  }

  drawFileTicker() {
    // Anchored along open-plan north wall interior, below the wall, above desks.
    // Left-aligned, single-line monospace, fades based on age of newest entry.
    const tx = OPEN_ROOM.x + 10;
    const ty = OPEN_ROOM.y + 20;
    this.fileTickerText = this.add
      .text(tx, ty, "", {
        fontFamily: "ui-monospace, Menlo, monospace",
        fontSize: "9px",
        color: "#cfe6ff",
        resolution: 2,
      })
      .setOrigin(0, 0)
      .setAlpha(0);
    this.fileTickerText.setDepth(5);
  }

  renderFileTicker() {
    if (!this.fileTickerText) return;
    if (!this.fileTicker.length) {
      this.fileTickerText.setText("");
      return;
    }
    const parts = this.fileTicker.map((e) => shortPath(e.path));
    // join and truncate to fit open-plan room width.
    const maxChars = Math.floor((OPEN_ROOM.w - 20) / AVG_CHAR_WIDTH_PX);
    let text = parts.join(" · ");
    if (text.length > maxChars) text = text.slice(0, Math.max(0, maxChars - 1)) + "…";
    this.fileTickerText.setText(text);
  }

  drawWindows() {
    for (const w of WINDOWS) this.drawWindow(w);
  }

  drawWindow(w) {
    // light spill — its own Graphics so the ambient loop can modulate alpha
    // without redrawing frame/glass/mullions each frame.
    if (w.spill && w.roomEdgeY != null) {
      const spill = this.add.graphics();
      const edgeY = w.roomEdgeY;
      const depth = 20;
      const farY = w.spill === "south" ? edgeY + depth : edgeY - depth;
      spill.fillStyle(PALETTE.windowGlass, 1);
      spill.fillPoints(
        [
          { x: w.x - WINDOW_W / 2, y: edgeY },
          { x: w.x + WINDOW_W / 2, y: edgeY },
          { x: w.x + (WINDOW_W / 2 + 12), y: farY },
          { x: w.x - (WINDOW_W / 2 + 12), y: farY },
        ],
        true
      );
      spill.setAlpha(WINDOW_SPILL_BASE_ALPHA);
      this.windowSpills.push(spill);
    }

    const g = this.add.graphics();
    const fx = w.x - WINDOW_W / 2;
    const fy = w.y - WINDOW_H / 2;

    // frame
    g.fillStyle(PALETTE.windowFrame, 1);
    g.fillRect(fx, fy, WINDOW_W, WINDOW_H);
    // glass
    g.fillStyle(PALETTE.windowGlass, 1);
    g.fillRect(fx + 2, fy + 1, WINDOW_W - 4, WINDOW_H - 2);
    // mullions
    g.lineStyle(1, PALETTE.windowFrame, 0.6);
    g.lineBetween(w.x, fy + 1, w.x, fy + WINDOW_H - 1);
    g.lineBetween(fx + 2, w.y, fx + WINDOW_W - 2, w.y);
    // top highlight
    g.lineStyle(1, PALETTE.wallHi, 0.5);
    g.lineBetween(fx, fy, fx + WINDOW_W, fy);
  }

  drawFloor() {
    const g = this.add.graphics();
    g.fillStyle(0x0b0d12, 1);
    g.fillRect(0, 0, WORLD_W, WORLD_H);

    const tile = 32;
    for (let y = OPEN_ROOM.y; y < OPEN_ROOM.y + OPEN_ROOM.h; y += tile) {
      for (let x = OPEN_ROOM.x; x < OPEN_ROOM.x + OPEN_ROOM.w; x += tile / 2) {
        const odd = (Math.floor(y / tile) + Math.floor(x / (tile / 2))) % 2;
        g.fillStyle(odd ? PALETTE.floorA : PALETTE.floorB, 1);
        g.fillRect(x, y, tile / 2, tile);
      }
    }
    g.lineStyle(1, PALETTE.floorLine, 0.4);
    for (let y = OPEN_ROOM.y + tile; y < OPEN_ROOM.y + OPEN_ROOM.h; y += tile) {
      g.lineBetween(OPEN_ROOM.x, y, OPEN_ROOM.x + OPEN_ROOM.w, y);
    }

    // meeting room: cool carpet
    for (let y = MEETING_ROOM.y; y < MEETING_ROOM.y + MEETING_ROOM.h; y += tile) {
      for (let x = MEETING_ROOM.x; x < MEETING_ROOM.x + MEETING_ROOM.w; x += tile) {
        const odd = (Math.floor(y / tile) + Math.floor(x / tile)) % 2;
        g.fillStyle(odd ? PALETTE.floorMeetingA : PALETTE.floorMeetingB, 1);
        g.fillRect(x, y, tile, tile);
      }
    }
    // lab room: clinical tile
    for (let y = LAB_ROOM.y; y < LAB_ROOM.y + LAB_ROOM.h; y += tile) {
      for (let x = LAB_ROOM.x; x < LAB_ROOM.x + LAB_ROOM.w; x += tile) {
        const odd = (Math.floor(y / tile) + Math.floor(x / tile)) % 2;
        g.fillStyle(odd ? PALETTE.floorLabA : PALETTE.floorLabB, 1);
        g.fillRect(x, y, tile, tile);
      }
    }
    g.lineStyle(1, 0x0e1c22, 0.6);
    for (let y = LAB_ROOM.y + tile; y < LAB_ROOM.y + LAB_ROOM.h; y += tile) {
      g.lineBetween(LAB_ROOM.x, y, LAB_ROOM.x + LAB_ROOM.w, y);
    }
    for (let x = LAB_ROOM.x + tile; x < LAB_ROOM.x + LAB_ROOM.w; x += tile) {
      g.lineBetween(x, LAB_ROOM.y, x, LAB_ROOM.y + LAB_ROOM.h);
    }
  }

  drawWalls() {
    const g = this.add.graphics();
    const T = 6;

    const drawRoomWalls = (room) => {
      g.fillStyle(PALETTE.wall, 1);
      g.fillRect(room.x - T, room.y - T, room.w + 2 * T, T);
      g.fillRect(room.x - T, room.y + room.h, room.w + 2 * T, T);
      g.fillRect(room.x - T, room.y - T, T, room.h + 2 * T);
      g.fillRect(room.x + room.w, room.y - T, T, room.h + 2 * T);

      g.fillStyle(PALETTE.wallHi, 1);
      g.fillRect(room.x - T, room.y - T, room.w + 2 * T, 2);
      g.fillRect(room.x - T, room.y - T, 2, room.h + 2 * T);
    };

    drawRoomWalls(OPEN_ROOM);
    drawRoomWalls(MEETING_ROOM);
    drawRoomWalls(LAB_ROOM);

    // exterior door (west wall of OPEN_ROOM)
    g.fillStyle(0x0b0d12, 1);
    g.fillRect(OPEN_ROOM.x - T, DOOR.y - DOOR.h / 2, T, DOOR.h);
    g.lineStyle(1, 0x6b84a8, 0.6);
    g.lineBetween(
      OPEN_ROOM.x - T - 2, DOOR.y - DOOR.h / 2,
      OPEN_ROOM.x - T - 2, DOOR.y + DOOR.h / 2
    );

    const punchInnerDoor = (door, accent) => {
      g.fillStyle(0x0b0d12, 1);
      const innerGapX = OPEN_ROOM.x + OPEN_ROOM.w - T;
      const totalSpan = MEETING_ROOM.x - (OPEN_ROOM.x + OPEN_ROOM.w) + 2 * T;
      g.fillRect(innerGapX, door.y - door.h / 2, totalSpan, door.h);
      g.lineStyle(1, accent, 0.7);
      g.lineBetween(
        OPEN_ROOM.x + OPEN_ROOM.w + T, door.y - door.h / 2,
        OPEN_ROOM.x + OPEN_ROOM.w + T, door.y + door.h / 2
      );
      g.lineBetween(
        MEETING_ROOM.x - T - 2, door.y - door.h / 2,
        MEETING_ROOM.x - T - 2, door.y + door.h / 2
      );
    };

    punchInnerDoor(MEETING_DOOR, 0x9ac8ff);
    punchInnerDoor(LAB_DOOR, 0x66ff88);

    // ambient tints
    g.fillStyle(0xd6c9a5, 0.08);
    g.fillEllipse(
      OPEN_ROOM.x + OPEN_ROOM.w / 2, OPEN_ROOM.y + OPEN_ROOM.h / 2,
      OPEN_ROOM.w * 0.8, OPEN_ROOM.h * 0.9
    );
    g.fillStyle(0x9ac8ff, 0.05);
    g.fillEllipse(
      MEETING_ROOM.x + MEETING_ROOM.w / 2, MEETING_ROOM.y + MEETING_ROOM.h / 2,
      MEETING_ROOM.w * 0.85, MEETING_ROOM.h * 0.9
    );
    g.fillStyle(0x66ff88, 0.05);
    g.fillEllipse(
      LAB_ROOM.x + LAB_ROOM.w / 2, LAB_ROOM.y + LAB_ROOM.h / 2,
      LAB_ROOM.w * 0.85, LAB_ROOM.h * 0.9
    );

    // room labels
    const label = (x, y, t, color) =>
      this.add
        .text(x, y, t, {
          fontFamily: "ui-monospace, Menlo, monospace",
          fontSize: "9px",
          color,
        })
        .setOrigin(1, 0);

    label(OPEN_ROOM.x + OPEN_ROOM.w - 10, OPEN_ROOM.y + 6, "OPEN PLAN", "#ffffff60");
    label(MEETING_ROOM.x + MEETING_ROOM.w - 10, MEETING_ROOM.y + 6, "MEETING ROOM", "#9ac8ff90");
    label(LAB_ROOM.x + LAB_ROOM.w - 10, LAB_ROOM.y + 6, "TEST LAB", "#66ff8890");
  }

  drawDecor() {
    const g = this.add.graphics();
    const potPositions = [
      { x: OPEN_ROOM.x + 30, y: OPEN_ROOM.y + 30 },
      { x: OPEN_ROOM.x + OPEN_ROOM.w - 30, y: OPEN_ROOM.y + 30 },
      { x: OPEN_ROOM.x + 30, y: OPEN_ROOM.y + OPEN_ROOM.h - 30 },
    ];
    for (const p of potPositions) {
      g.fillStyle(PALETTE.shadow, 0.3);
      g.fillEllipse(p.x + 2, p.y + 14, 22, 6);
      g.fillStyle(PALETTE.plantPot, 1);
      g.fillRoundedRect(p.x - 10, p.y, 20, 14, 2);
      g.fillStyle(PALETTE.plant, 1);
      g.fillCircle(p.x - 5, p.y - 4, 8);
      g.fillCircle(p.x + 5, p.y - 2, 7);
      g.fillCircle(p.x, p.y - 9, 7);
      g.fillStyle(0x62c277, 1);
      g.fillCircle(p.x - 3, p.y - 8, 4);
      g.fillCircle(p.x + 4, p.y - 4, 4);
    }
  }

  drawDesk(d) {
    const g = this.add.graphics();
    const x = d.x - DESK_W / 2;
    const y = d.y - DESK_H / 2;

    g.fillStyle(PALETTE.shadow, 0.28);
    g.fillEllipse(d.x, d.y + DESK_H / 2 + 4, DESK_W * 0.95, 8);

    g.fillStyle(PALETTE.deskShade, 1);
    g.fillRoundedRect(x, y + DESK_H - 6, DESK_W, 6, 2);
    g.fillStyle(PALETTE.deskTop, 1);
    g.fillRoundedRect(x, y, DESK_W, DESK_H - 4, 2);
    g.lineStyle(1, PALETTE.deskEdge, 0.9);
    g.strokeRoundedRect(x, y, DESK_W, DESK_H - 4, 2);

    g.lineStyle(1, PALETTE.deskEdge, 0.18);
    for (let py = y + 6; py < y + DESK_H - 8; py += 6) {
      g.lineBetween(x + 3, py, x + DESK_W - 3, py);
    }

    const mw = 34, mh = 16;
    const mx = d.x - mw / 2;
    const my = y + 3;
    g.fillStyle(0x2a2f39, 1);
    g.fillRect(d.x - 4, my + mh, 8, 3);
    g.fillRect(d.x - 8, my + mh + 3, 16, 2);
    g.fillStyle(0x1a1f28, 1);
    g.fillRoundedRect(mx - 1, my - 1, mw + 2, mh + 2, 2);
    g.fillStyle(PALETTE.monitor, 1);
    g.fillRect(mx, my, mw, mh);
    g.fillStyle(PALETTE.monitorGlow, 0.8);
    g.fillRect(mx + 2, my + 2, mw - 4, mh - 4);
    g.fillStyle(0xffffff, 0.45);
    g.fillRect(mx + 3, my + 3, 8, 1);
    g.fillRect(mx + 3, my + 5, 16, 1);
    g.fillRect(mx + 3, my + 7, 12, 1);
    g.fillRect(mx + 3, my + 9, 18, 1);
    g.fillRect(mx + 3, my + 11, 10, 1);
    g.fillRect(mx + 3, my + 13, 14, 1);

    const kw = 36, kh = 7;
    const kx = d.x - kw / 2;
    const ky = y + DESK_H - 14;
    g.fillStyle(PALETTE.keyboard, 1);
    g.fillRoundedRect(kx, ky, kw, kh, 1);
    g.lineStyle(1, 0x0a0d11, 0.8);
    for (let i = 1; i < 12; i++) {
      const lx = kx + (kw / 12) * i;
      g.lineBetween(lx, ky + 1, lx, ky + kh - 1);
    }

    g.fillStyle(PALETTE.mouse, 1);
    g.fillRoundedRect(d.x + 22, y + DESK_H - 13, 7, 5, 2);

    g.fillStyle(0xffffff, 1);
    g.fillRoundedRect(x + 6, y + DESK_H - 16, 8, 9, 1);
    g.fillStyle(0x4a3221, 1);
    g.fillRect(x + 7, y + DESK_H - 15, 6, 2);
    g.lineStyle(1, 0xcfd6df, 1);
    g.strokeRect(x + 14, y + DESK_H - 14, 2, 4);

    g.fillStyle(PALETTE.monitorGlow, 0.12);
    g.fillCircle(d.x, my + mh / 2, 36);

    const chairY = d.y + DESK_H / 2 + 16;
    g.fillStyle(PALETTE.shadow, 0.3);
    g.fillEllipse(d.x, chairY + 6, 22, 5);
    g.fillStyle(PALETTE.chairHi, 1);
    g.fillRoundedRect(d.x - 12, chairY - 10, 24, 4, 1);
    g.fillStyle(PALETTE.chair, 1);
    g.fillCircle(d.x, chairY, 9);
    g.lineStyle(1, PALETTE.chairHi, 1);
    g.strokeCircle(d.x, chairY, 9);
    g.fillStyle(PALETTE.chairHi, 1);
    g.fillCircle(d.x, chairY, 2);
  }

  drawMeetingRoom() {
    const g = this.add.graphics();

    // whiteboard on north wall
    const wbW = 200, wbH = 22;
    const wbX = MEETING_ROOM.x + (MEETING_ROOM.w - wbW) / 2;
    const wbY = MEETING_ROOM.y + 10;
    g.fillStyle(PALETTE.whiteboardFrame, 1);
    g.fillRoundedRect(wbX - 3, wbY - 3, wbW + 6, wbH + 6, 2);
    g.fillStyle(PALETTE.whiteboardBody, 1);
    g.fillRoundedRect(wbX, wbY, wbW, wbH, 1);
    g.lineStyle(1.5, 0xff5c5c, 1);
    g.strokeRect(wbX + 12, wbY + 5, 32, 12);
    g.lineStyle(1, 0x222222, 1);
    g.lineBetween(wbX + 52, wbY + 7, wbX + 88, wbY + 7);
    g.lineBetween(wbX + 52, wbY + 11, wbX + 80, wbY + 11);
    g.lineBetween(wbX + 52, wbY + 15, wbX + 96, wbY + 15);
    g.lineStyle(1.5, 0x4aa35a, 1);
    g.beginPath();
    g.arc(wbX + 130, wbY + 11, 5, 0, Math.PI * 2);
    g.strokePath();
    g.lineStyle(1, 0x3a78ff, 1);
    g.lineBetween(wbX + 142, wbY + 4, wbX + 178, wbY + 18);
    g.lineBetween(wbX + 178, wbY + 4, wbX + 142, wbY + 18);
    g.fillStyle(PALETTE.whiteboardFrame, 1);
    g.fillRect(wbX, wbY + wbH + 3, wbW, 2);
    g.fillStyle(0xff5c5c, 1);
    g.fillRect(wbX + 24, wbY + wbH + 1, 12, 2);
    g.fillStyle(0x3a78ff, 1);
    g.fillRect(wbX + 60, wbY + wbH + 1, 12, 2);
    g.fillStyle(0x4aa35a, 1);
    g.fillRect(wbX + 96, wbY + wbH + 1, 12, 2);

    // Live whiteboard text layer — hidden by default; revealed when the
    // session sim is seated in the meeting room and has a session_prompt.
    this.whiteboardRect = { x: wbX, y: wbY, w: wbW, h: wbH };
    this.whiteboardText = this.add
      .text(wbX + 4, wbY + 3, "", {
        fontFamily: "ui-monospace, Menlo, monospace",
        fontSize: "9px",
        color: "#1a1f28",
        wordWrap: { width: wbW - 8 },
        resolution: 2,
        lineSpacing: 1,
      })
      .setOrigin(0, 0)
      .setAlpha(0)
      .setDepth(4);

    // conference table (smaller)
    const tx = TABLE.cx - TABLE.w / 2;
    const ty = TABLE.cy - TABLE.h / 2;
    g.fillStyle(PALETTE.shadow, 0.3);
    g.fillEllipse(TABLE.cx, TABLE.cy + TABLE.h / 2 + 5, TABLE.w * 0.95, 10);
    g.fillStyle(PALETTE.deskShade, 1);
    g.fillRoundedRect(tx, ty + TABLE.h - 6, TABLE.w, 6, 4);
    g.fillStyle(PALETTE.deskTop, 1);
    g.fillRoundedRect(tx, ty, TABLE.w, TABLE.h - 4, 4);
    g.lineStyle(1, PALETTE.deskEdge, 0.9);
    g.strokeRoundedRect(tx, ty, TABLE.w, TABLE.h - 4, 4);
    g.lineStyle(1, PALETTE.deskEdge, 0.2);
    for (let py = ty + 8; py < ty + TABLE.h - 10; py += 8) {
      g.lineBetween(tx + 4, py, tx + TABLE.w - 4, py);
    }

    // table props
    g.fillStyle(0xf2f2ee, 1);
    g.fillRect(tx + 12, ty + 10, 18, 12);
    g.fillRect(tx + TABLE.w - 30, ty + 10, 18, 12);
    g.lineStyle(1, 0x808088, 1);
    g.lineBetween(tx + 14, ty + 14, tx + 28, ty + 14);
    g.lineBetween(tx + TABLE.w - 28, ty + 14, tx + TABLE.w - 14, ty + 14);
    // carafe
    g.fillStyle(0x2a2f39, 1);
    g.fillRoundedRect(TABLE.cx - 6, TABLE.cy - 10, 12, 20, 3);
    g.fillStyle(0x4a3221, 1);
    g.fillRect(TABLE.cx - 4, TABLE.cy - 7, 8, 10);
    g.fillStyle(0xcfd6df, 1);
    g.fillRect(TABLE.cx - 8, TABLE.cy - 4, 4, 5);

    for (const seat of meetingSeats) this.drawMeetingChair(seat);
  }

  drawMeetingChair(seat) {
    const g = this.add.graphics();
    const cy = seat.y;
    g.fillStyle(PALETTE.shadow, 0.3);
    g.fillEllipse(seat.x, cy + 6, 22, 5);
    if (seat.side === "north") {
      g.fillStyle(PALETTE.chairHi, 1);
      g.fillRoundedRect(seat.x - 12, cy + 6, 24, 4, 1);
    } else {
      g.fillStyle(PALETTE.chairHi, 1);
      g.fillRoundedRect(seat.x - 12, cy - 10, 24, 4, 1);
    }
    g.fillStyle(PALETTE.chair, 1);
    g.fillCircle(seat.x, cy, 9);
    g.lineStyle(1, PALETTE.chairHi, 1);
    g.strokeCircle(seat.x, cy, 9);
    g.fillStyle(PALETTE.chairHi, 1);
    g.fillCircle(seat.x, cy, 2);
  }

  drawLabRoom() {
    const g = this.add.graphics();

    // BUILD ✓ board on west wall (just inside room)
    const sgX = LAB_ROOM.x + 8;
    const sgY = LAB_ROOM.y + LAB_ROOM.h / 2 - 18;
    g.fillStyle(0x10181c, 1);
    g.fillRoundedRect(sgX, sgY, 36, 36, 2);
    g.lineStyle(1, PALETTE.led, 1);
    g.strokeRoundedRect(sgX, sgY, 36, 36, 2);
    const board = this.add
      .text(sgX + 18, sgY + 4, "BUILD\n  ✓", {
        fontFamily: "ui-monospace, Menlo, monospace",
        fontSize: "10px",
        color: "#66ff88",
        align: "center",
        resolution: 2,
      })
      .setOrigin(0.5, 0);
    board.setShadow(0, 0, "#66ff88", 6, true, true);

    // server rack in the SE corner
    const rackX = LAB_ROOM.x + LAB_ROOM.w - 30;
    const rackY = LAB_ROOM.y + LAB_ROOM.h - 70;
    g.fillStyle(PALETTE.shadow, 0.3);
    g.fillEllipse(rackX + 8, rackY + 56, 28, 6);
    g.fillStyle(0x14181f, 1);
    g.fillRoundedRect(rackX, rackY, 22, 52, 2);
    g.lineStyle(1, 0x2a313d, 1);
    g.strokeRoundedRect(rackX, rackY, 22, 52, 2);
    for (let i = 0; i < 5; i++) {
      const ry = rackY + 4 + i * 9;
      g.fillStyle(0x1f2630, 1);
      g.fillRect(rackX + 3, ry, 16, 6);
      // LEDs
      const onA = ((i + Math.floor(performance.now() / 800)) % 2) === 0;
      g.fillStyle(onA ? PALETTE.led : PALETTE.ledOff, 1);
      g.fillRect(rackX + 5, ry + 2, 2, 2);
      g.fillStyle(0xffaa55, 1);
      g.fillRect(rackX + 9, ry + 2, 2, 2);
    }

    // workbench across north wall
    const bx = BENCH.x;
    const by = BENCH.y;
    g.fillStyle(PALETTE.shadow, 0.28);
    g.fillEllipse(bx + BENCH.w / 2, by + BENCH.h + 4, BENCH.w * 0.95, 8);
    g.fillStyle(PALETTE.benchShade, 1);
    g.fillRoundedRect(bx, by + BENCH.h - 6, BENCH.w, 6, 2);
    g.fillStyle(PALETTE.benchTop, 1);
    g.fillRoundedRect(bx, by, BENCH.w, BENCH.h - 4, 2);
    g.lineStyle(1, PALETTE.benchEdge, 0.9);
    g.strokeRoundedRect(bx, by, BENCH.w, BENCH.h - 4, 2);

    // 3 stations on the bench
    for (const cx of LAB_STATION_XS) this.drawLabStation(cx, by);

    // chairs south of bench, in front of each station
    for (const seat of labStations) this.drawLabChair(seat);
  }

  drawLabStation(cx, by) {
    const g = this.add.graphics();
    // small monitor
    const mw = 26, mh = 14;
    const mx = cx - mw / 2;
    const my = by + 4;
    g.fillStyle(0x2a2f39, 1);
    g.fillRect(cx - 3, my + mh, 6, 2);
    g.fillRect(cx - 6, my + mh + 2, 12, 2);
    g.fillStyle(0x1a1f28, 1);
    g.fillRoundedRect(mx - 1, my - 1, mw + 2, mh + 2, 1);
    g.fillStyle(PALETTE.monitor, 1);
    g.fillRect(mx, my, mw, mh);
    // tint overlay Graphics — separate layer so pass/fail flashes don't
    // disturb the static scope trace art drawn below.
    const tint = this.add.graphics();
    tint.setAlpha(0);
    const station = labStations.find((s) => s.x === cx);
    if (station) {
      station.monitorG = tint;
      station.monitorRect = { mx, my, mw, mh };
    }
    // green test output
    g.fillStyle(0x0c2014, 1);
    g.fillRect(mx + 1, my + 1, mw - 2, mh - 2);
    g.fillStyle(PALETTE.scopeTrace, 0.9);
    g.fillRect(mx + 2, my + 3, 6, 1);
    g.fillRect(mx + 2, my + 5, 14, 1);
    g.fillRect(mx + 2, my + 7, 9, 1);
    g.fillRect(mx + 2, my + 9, 17, 1);
    g.fillRect(mx + 2, my + 11, 11, 1);
    g.fillStyle(0xffaa55, 0.9);
    g.fillRect(mx + 18, my + 3, 4, 1);

    // oscilloscope to the right of the monitor
    const ow = 20, oh = 14;
    const ox = cx + mw / 2 + 6;
    const oy = my;
    g.fillStyle(PALETTE.scope, 1);
    g.fillRoundedRect(ox, oy, ow, oh, 1);
    g.lineStyle(1, PALETTE.benchEdge, 1);
    g.strokeRoundedRect(ox, oy, ow, oh, 1);
    // scope grid
    g.lineStyle(1, 0x1a3328, 1);
    for (let i = 1; i < 4; i++) {
      g.lineBetween(ox + (ow / 4) * i, oy + 1, ox + (ow / 4) * i, oy + oh - 1);
      g.lineBetween(ox + 1, oy + (oh / 4) * i, ox + ow - 1, oy + (oh / 4) * i);
    }
    // sine trace
    g.lineStyle(1, PALETTE.scopeTrace, 1);
    let prevX = ox + 1;
    let prevY = oy + oh / 2;
    for (let xi = 1; xi <= ow - 2; xi += 1) {
      const yi = oy + oh / 2 + Math.sin(xi * 0.6) * (oh / 3);
      g.lineBetween(prevX, prevY, ox + xi, yi);
      prevX = ox + xi;
      prevY = yi;
    }

    // little gadget under the bench front-edge
    g.fillStyle(0x2a2f39, 1);
    g.fillRoundedRect(cx - 18, by + BENCH.h - 14, 12, 6, 1);
    g.fillStyle(PALETTE.led, 1);
    g.fillRect(cx - 16, by + BENCH.h - 12, 2, 2);
    g.fillStyle(0xffaa55, 1);
    g.fillRect(cx - 12, by + BENCH.h - 12, 2, 2);
  }

  drawLabChair(seat) {
    const g = this.add.graphics();
    const cy = seat.y;
    g.fillStyle(PALETTE.shadow, 0.3);
    g.fillEllipse(seat.x, cy + 6, 22, 5);
    g.fillStyle(PALETTE.chairHi, 1);
    g.fillRoundedRect(seat.x - 12, cy - 10, 24, 4, 1);
    g.fillStyle(PALETTE.chair, 1);
    g.fillCircle(seat.x, cy, 9);
    g.lineStyle(1, PALETTE.chairHi, 1);
    g.strokeCircle(seat.x, cy, 9);
    g.fillStyle(PALETTE.chairHi, 1);
    g.fillCircle(seat.x, cy, 2);
  }

  buildSimTextures() {
    const g = this.make.graphics({ x: 0, y: 0, add: false });

    g.clear();
    g.fillStyle(0xffffff, 1);
    g.fillRoundedRect(1, 3, 12, 12, 2);
    g.fillRect(0, 12, 14, 4);
    g.generateTexture("sim-body", 14, 16);

    g.clear();
    g.fillStyle(0xffffff, 1);
    g.fillRoundedRect(0, 0, 10, 10, 3);
    g.generateTexture("sim-head", 10, 10);

    g.clear();
    g.fillStyle(0xffffff, 1);
    g.fillRoundedRect(0, 0, 10, 6, 3);
    g.fillRect(0, 2, 10, 4);
    g.generateTexture("sim-hair", 10, 6);

    g.clear();
    g.fillStyle(0xffffff, 1);
    g.fillRect(1, 0, 4, 5);
    g.fillRect(7, 0, 4, 5);
    g.generateTexture("sim-legs", 12, 5);

    g.clear();
    g.fillStyle(0x000000, 1);
    g.fillEllipse(12, 4, 22, 7);
    g.generateTexture("sim-shadow", 24, 8);

    g.destroy();
  }

  makeSim(user, opts = {}) {
    const shirt = shirtColor(user);
    const pants = pantsColor(user);
    const skin = skinColor(user);
    const container = this.add.container(0, 0);

    const shadow = this.add.image(0, 16, "sim-shadow").setAlpha(0.45);
    const legs = this.add.image(0, 12, "sim-legs").setTint(pants);
    const body = this.add.image(0, 2, "sim-body").setTint(shirt);
    const head = this.add.image(0, -9, "sim-head").setTint(skin);
    const hair = this.add.image(0, -12, "sim-hair").setTint(pants);

    container.add([shadow, legs, body, head, hair]);

    if (opts.badge) {
      const badge = this.add
        .text(0, -20, opts.badge, {
          fontFamily: "system-ui, sans-serif",
          fontSize: "10px",
          color: "#ffffff",
          resolution: 2,
        })
        .setOrigin(0.5, 1);
      container.add(badge);
      container.badge = badge;
    }

    container.setSize(14, 28);
    container.shadow = shadow;
    container.legs = legs;
    container.body = body;
    container.head = head;
    container.hair = hair;
    return container;
  }

  connect() {
    const statusEl = document.getElementById("status");
    const open = () => {
      const es = new EventSource("/events");
      es.onopen = () => (statusEl.textContent = "connected");
      es.onerror = () => {
        statusEl.textContent = "reconnecting…";
        es.close();
        setTimeout(open, 2000);
      };
      es.onmessage = (ev) => this.onEvent(JSON.parse(ev.data));
    };
    open();
  }

  onEvent(msg) {
    this.recentEvents.push(Date.now());
    if (msg.type === "snapshot") {
      for (const [id, sim] of this.sims) {
        this.releaseTarget(sim);
        this.destroySim(sim);
        this.sims.delete(id);
      }
      for (const a of msg.agents) {
        this.spawnSim(a, { immediate: true });
        if (a.visit && a.visit.room && a.visit.until > Date.now()) {
          const sim = this.sims.get(a.agent_id);
          if (sim) this.visitTo(sim, a.visit.room);
        }
      }
    } else if (msg.type === "start") {
      this.spawnSim(msg.agent, { immediate: false });
    } else if (msg.type === "stop") {
      const sim = this.sims.get(msg.agent_id);
      if (sim) this.dismissSim(sim);
    } else if (msg.type === "visit") {
      const sim = this.sims.get(msg.agent_id);
      if (!sim) return;
      if (msg.room) this.visitTo(sim, msg.room);
      else this.returnFromVisit(sim);
    } else if (msg.type === "tool") {
      this.spawnMote(msg.agent_id, msg.tool_name);
    } else if (msg.type === "reclassify") {
      const sim = this.sims.get(msg.agent_id);
      if (sim) this.reclassifySim(sim, msg.permission_mode);
    } else if (msg.type === "file-touch") {
      this.pushFileTouch(msg.path);
    } else if (msg.type === "bash-result") {
      this.flashLabMonitor(!!msg.ok);
    } else if (msg.type === "prompt") {
      const sim = this.sims.get(msg.agent_id);
      if (sim) {
        sim.sessionPrompt = msg.text || "";
        sim.idle = false;
        this.refreshWhiteboard();
      }
    } else if (msg.type === "idle") {
      const sim = this.sims.get(msg.agent_id);
      if (sim) sim.idle = !!msg.idle;
    } else if (msg.type === "turn-end") {
      const sim = this.sims.get(msg.agent_id);
      if (sim) this.spawnTurnEndWave(sim);
    } else if (msg.type === "tool-error") {
      const sim = this.sims.get(msg.agent_id);
      if (sim) this.spawnErrorHalo(sim);
    }
  }

  // Choose the sim whose sessionPrompt should populate the whiteboard. Prefer
  // the session sim (agent_type === "claude") when one is seated in meeting;
  // otherwise fall back to any sim seated in meeting with a prompt.
  pickWhiteboardSim() {
    let fallback = null;
    for (const sim of this.sims.values()) {
      if (!sim.seated) continue;
      if (!sim.target || sim.target.kind !== "meeting") continue;
      if (!sim.sessionPrompt) continue;
      if (sim.agent && sim.agent.agent_type === "claude") return sim;
      if (!fallback) fallback = sim;
    }
    return fallback;
  }

  refreshWhiteboard() {
    if (!this.whiteboardText) return;
    const sim = this.pickWhiteboardSim();
    if (sim) {
      this.whiteboardText.setText(sim.sessionPrompt);
      this.tweens.killTweensOf(this.whiteboardText);
      this.tweens.add({
        targets: this.whiteboardText,
        alpha: 0.9,
        duration: 220,
        ease: "sine.out",
      });
    } else {
      this.tweens.killTweensOf(this.whiteboardText);
      this.tweens.add({
        targets: this.whiteboardText,
        alpha: 0,
        duration: 220,
        ease: "sine.in",
        onComplete: () => this.whiteboardText.setText(""),
      });
    }
  }

  spawnTurnEndWave(sim) {
    if (!sim || !sim.seated) return;
    if (!sim.sprite || !sim.sprite.head) return;
    const head = sim.sprite.head;
    this.tweens.killTweensOf(head);
    head.x = 0;
    this.tweens.add({
      targets: head,
      x: 1,
      duration: 120,
      yoyo: true,
      repeat: 3,
      ease: "sine.inOut",
      onYoyo: () => {
        head.x = -1;
      },
      onComplete: () => {
        head.x = 0;
      },
    });
  }

  spawnErrorHalo(sim) {
    if (!sim || !sim.sprite) return;
    sim.errorUntil = performance.now() + 2000;
    if (sim.haloG) {
      this.tweens.killTweensOf(sim.haloG);
      sim.haloG.destroy();
      sim.haloG = null;
    }
    const g = this.add.graphics();
    g.setDepth(6);
    g.lineStyle(2, 0xff6464, 0.6);
    g.strokeCircle(0, 0, 18);
    g.x = sim.sprite.x;
    g.y = sim.sprite.y;
    sim.haloG = g;
    this.tweens.add({
      targets: g,
      alpha: 0,
      duration: 2000,
      ease: "sine.out",
      onUpdate: () => {
        if (sim.sprite && sim.sprite.active) {
          g.x = sim.sprite.x;
          g.y = sim.sprite.y;
        }
      },
      onComplete: () => {
        g.destroy();
        if (sim.haloG === g) sim.haloG = null;
        sim.errorUntil = 0;
      },
    });
  }

  pushFileTouch(path) {
    if (!path) return;
    this.fileTicker.unshift({ path, t: performance.now() });
    if (this.fileTicker.length > 8) this.fileTicker.length = 8;
    this.renderFileTicker();
  }

  flashLabMonitor(ok) {
    const station = labStations[this.labMonitorCursor % labStations.length];
    this.labMonitorCursor = (this.labMonitorCursor + 1) % labStations.length;
    if (!station || !station.monitorG) return;
    const g = station.monitorG;
    // kill any in-flight tween on this graphics to avoid stacking.
    this.tweens.killTweensOf(g);
    g.setAlpha(0.6);
    g.clear();
    const { mx, my, mw, mh } = station.monitorRect;
    g.fillStyle(ok ? 0x6ef08e : 0xff6464, 1);
    g.fillRect(mx, my, mw, mh);
    this.tweens.add({
      targets: g,
      alpha: 0,
      duration: 800,
      ease: "sine.out",
    });
  }

  spawnMote(agentId, toolName) {
    const sim = this.sims.get(agentId);
    if (!sim) return;
    while (this.motes.length >= MOTE_CAP) {
      const oldest = this.motes.shift();
      if (oldest?.rect) oldest.rect.destroy();
    }
    const color = MOTE_COLORS[toolName] ?? MOTE_DEFAULT_COLOR;
    const rect = this.add
      .rectangle(sim.sprite.x, sim.sprite.y - 18, 2, 2, color, 1)
      .setAlpha(0.9);
    const mote = { rect, active: true };
    this.motes.push(mote);
    this.tweens.add({
      targets: rect,
      y: rect.y - 24,
      alpha: 0,
      duration: MOTE_LIFETIME_MS,
      ease: "sine.out",
      onComplete: () => {
        rect.destroy();
        mote.active = false;
      },
    });
  }

  classify(agent) {
    if (isLabAgent(agent)) return "test";
    if (agent.permission_mode === "plan") return "plan";
    return "default";
  }

  pickTarget(agent) {
    const kind = this.classify(agent);
    if (kind === "test") {
      const station = findFreeLabStation();
      if (station) {
        station.taken = true;
        return { kind: "lab", station };
      }
      const spot = LAB_QUEUE_SPOTS[this.queuedLabOverflow++ % LAB_QUEUE_SPOTS.length];
      return { kind: "lab-queue", x: spot.x, y: spot.y };
    }
    if (kind === "plan") {
      const seat = findFreeMeetingSeat();
      if (seat) {
        seat.taken = true;
        return { kind: "meeting", seat };
      }
      const spot = MEETING_QUEUE_SPOTS[this.queuedMeetingOverflow++ % MEETING_QUEUE_SPOTS.length];
      return { kind: "meeting-queue", x: spot.x, y: spot.y };
    }
    const desk = findFreeDesk();
    if (desk) {
      desk.taken = true;
      return { kind: "desk", desk };
    }
    const spot = QUEUE_SPOTS[this.queuedOverflow++ % QUEUE_SPOTS.length];
    return { kind: "queue", x: spot.x, y: spot.y };
  }

  releaseTargetEntry(target) {
    if (!target) return;
    if (target.kind === "desk") target.desk.taken = false;
    if (target.kind === "meeting") target.seat.taken = false;
    if (target.kind === "lab") target.station.taken = false;
  }

  releaseTarget(sim) {
    this.releaseTargetEntry(sim.target);
    this.releaseTargetEntry(sim.homeTarget);
    sim.homeTarget = null;
  }

  pickLabTarget() {
    const station = findFreeLabStation();
    if (station) {
      station.taken = true;
      return { kind: "lab", station };
    }
    const spot = LAB_QUEUE_SPOTS[this.queuedLabOverflow++ % LAB_QUEUE_SPOTS.length];
    return { kind: "lab-queue", x: spot.x, y: spot.y };
  }

  stopMotion(sim) {
    if (sim.walkTween) {
      sim.walkTween.stop();
      sim.walkTween = null;
    }
    if (sim.bobTween) {
      sim.bobTween.stop();
      sim.bobTween = null;
    }
    sim.seated = false;
    sim.label.setAlpha(1);
  }

  moveToTarget(sim, newTarget) {
    const wasInMeeting =
      sim.target && sim.target.kind === "meeting" && sim.seated;
    this.stopMotion(sim);
    sim.target = newTarget;
    if (wasInMeeting && this.whiteboardText) this.refreshWhiteboard();
    this.walkPath(sim, this.computeRoute(sim, newTarget), () => {
      const seated =
        newTarget.kind === "desk" ||
        newTarget.kind === "meeting" ||
        newTarget.kind === "lab";
      if (seated) this.startBob(sim);
    });
  }

  visitTo(sim, room) {
    if (room !== "test") return;
    if (sim.visiting) return;
    if (!sim.target) return;
    if (sim.target.kind === "lab" || sim.target.kind === "lab-queue") return;
    const visitTarget = this.pickLabTarget();
    sim.homeTarget = sim.target;
    sim.visiting = true;
    this.moveToTarget(sim, visitTarget);
  }

  returnFromVisit(sim) {
    if (!sim.visiting || !sim.homeTarget) return;
    this.releaseTargetEntry(sim.target);
    const home = sim.homeTarget;
    sim.homeTarget = null;
    sim.visiting = false;
    this.moveToTarget(sim, home);
  }

  reclassifySim(sim, newMode) {
    if (!sim.agent) return;
    if (sim.visiting) {
      // Visit is a higher-priority temporary state; just update the stored
      // mode so returnFromVisit lands in the right place next time.
      sim.agent.permission_mode = newMode;
      return;
    }
    sim.agent.permission_mode = newMode;
    const newKind = this.classify(sim.agent);
    if (newKind === sim.kind) return;
    const oldTarget = sim.target;
    const newTarget = this.pickTarget(sim.agent);
    sim.kind = newKind;
    this.releaseTargetEntry(oldTarget);
    this.moveToTarget(sim, newTarget);
  }

  spawnSim(agent, { immediate }) {
    if (this.sims.has(agent.agent_id)) return;
    const kind = this.classify(agent);
    const target = this.pickTarget(agent);

    const badge = kind === "plan" ? "📋" : kind === "test" ? "🧪" : null;
    const sprite = this.makeSim(agent.user || "unknown", { badge });
    sprite.setScale(1.8);

    const restingPos = this.targetRestingPosition(target);
    const startX = immediate ? restingPos.x : OUTSIDE_X;
    const startY = immediate ? restingPos.y : DOOR.y;
    sprite.setPosition(startX, startY);

    const userTag = `${agent.user}@${truncate(agent.host || "", 14)}`;
    const modeTag = kind === "plan" ? " · plan" : kind === "test" ? " · test" : "";
    const labelColor = kind === "plan" ? "#cbe2ff" : kind === "test" ? "#c2ffd6" : "#f6f7f9";
    const taskTag = truncate(`${agent.agent_type} · ${agent.description || ""}`, 38);
    const label = this.add
      .text(startX, startY + 32, `${userTag}${modeTag}\n${taskTag}`, {
        fontFamily: "ui-monospace, Menlo, monospace",
        fontSize: "9px",
        color: labelColor,
        backgroundColor: "#000b",
        padding: { x: 4, y: 2 },
        align: "center",
        resolution: 2,
        lineSpacing: 1,
      })
      .setOrigin(0.5, 0)
      .setAlpha(0.95);

    sprite.setSize(28, 52);
    sprite.setInteractive(
      new Phaser.Geom.Rectangle(-14, -26, 28, 52),
      Phaser.Geom.Rectangle.Contains
    );
    sprite.on("pointerover", () => label.setAlpha(1).setDepth(10));
    sprite.on("pointerout", () => {
      if (sim && sim.seated) label.setAlpha(0.35);
    });

    const glyph = this.add
      .text(startX, startY - 36, "", {
        fontFamily: "ui-monospace, Menlo, monospace",
        fontSize: "12px",
        color: "#ffffff",
        resolution: 2,
      })
      .setOrigin(0.5, 0.5)
      .setAlpha(0.9);

    const sim = {
      id: agent.agent_id,
      agent,
      sprite,
      label,
      glyph,
      target,
      homeTarget: null,
      visiting: false,
      kind,
      bobTween: null,
      walkTween: null,
      seated: false,
      seatedAt: 0,
      footsteps: [],
      lastStepAt: 0,
      shirtHex: shirtColor(agent.user || "unknown"),
      sessionId: agent.session_id ?? null,
    };
    this.sims.set(agent.agent_id, sim);

    if (agent.agent_id !== agent.session_id && agent.session_id) {
      const parent = this.sims.get(agent.session_id);
      if (parent) {
        this.tethers.push({
          fromId: parent.id,
          toId: sim.id,
          bornAt: performance.now(),
        });
      }
    }

    if (immediate) this.startBob(sim);
    else this.walkIn(sim);
  }

  targetRestingPosition(target) {
    if (target.kind === "desk") return { x: target.desk.seatX, y: target.desk.seatY };
    if (target.kind === "meeting") return { x: target.seat.seatX, y: target.seat.seatY };
    if (target.kind === "lab") return { x: target.station.seatX, y: target.station.seatY };
    return { x: target.x, y: target.y };
  }

  pathFromDoorTo(target) {
    const insideOpenDoor = { x: HALLWAY_LEFT_X, y: DOOR.y };
    const staging = stagingForTarget(target);
    return [
      insideOpenDoor,
      ...pathBetweenHallNodes(insideOpenDoor, staging),
      ...targetApproachWaypoints(target),
    ];
  }

  routeFromCurrentPosition(cx, cy) {
    // Outside (west of the open room): enter via the west door.
    if (cx < OPEN_ROOM.x) {
      const handoff = { x: HALLWAY_LEFT_X, y: DOOR.y };
      return {
        prefix: [{ x: OPEN_ROOM.x + 2, y: DOOR.y }, handoff],
        handoff,
      };
    }
    // Inside meeting room: exit via meeting door, then snap to a corridor.
    if (
      cx >= MEETING_ROOM.x &&
      cx <= MEETING_ROOM.x + MEETING_ROOM.w &&
      cy >= MEETING_ROOM.y &&
      cy <= MEETING_ROOM.y + MEETING_ROOM.h
    ) {
      const corrY = nearestCorridorY(MEETING_DOOR.y);
      const handoff = { x: HALLWAY_RIGHT_X, y: corrY };
      return {
        prefix: [
          { x: MEETING_ROOM.x + 24, y: MEETING_DOOR.y },
          { x: HALLWAY_RIGHT_X, y: MEETING_DOOR.y },
          handoff,
        ],
        handoff,
      };
    }
    // Inside lab room: symmetrical.
    if (
      cx >= LAB_ROOM.x &&
      cx <= LAB_ROOM.x + LAB_ROOM.w &&
      cy >= LAB_ROOM.y &&
      cy <= LAB_ROOM.y + LAB_ROOM.h
    ) {
      const corrY = nearestCorridorY(LAB_DOOR.y);
      const handoff = { x: HALLWAY_RIGHT_X, y: corrY };
      return {
        prefix: [
          { x: LAB_ROOM.x + 24, y: LAB_DOOR.y },
          { x: HALLWAY_RIGHT_X, y: LAB_DOOR.y },
          handoff,
        ],
        handoff,
      };
    }
    // In the open room (at a desk seat, a queue spot, or walking): step south
    // or north to the nearest corridor, then west to the left hall unless
    // already close to the right hall.
    const corrY = nearestCorridorY(cy);
    const hallX = cx > HALLWAY_RIGHT_X - 20 ? HALLWAY_RIGHT_X : HALLWAY_LEFT_X;
    const prefix = [];
    if (Math.abs(cy - corrY) > 6) prefix.push({ x: cx, y: corrY });
    if (Math.abs(cx - hallX) > 6) prefix.push({ x: hallX, y: corrY });
    return { prefix, handoff: { x: hallX, y: corrY } };
  }

  computeRoute(sim, target) {
    const { prefix, handoff } = this.routeFromCurrentPosition(sim.sprite.x, sim.sprite.y);
    const staging = stagingForTarget(target);
    return [
      ...prefix,
      ...pathBetweenHallNodes(handoff, staging),
      ...targetApproachWaypoints(target),
    ];
  }

  pathToDoorFrom(target) {
    return this.pathFromDoorTo(target).slice(0, -1).reverse().concat([
      { x: OPEN_ROOM.x - 8, y: DOOR.y },
      { x: OUTSIDE_X, y: DOOR.y },
    ]);
  }

  walkIn(sim) {
    this.walkPath(sim, this.computeRoute(sim, sim.target), () => {
      const seated =
        sim.target.kind === "desk" ||
        sim.target.kind === "meeting" ||
        sim.target.kind === "lab";
      if (seated) this.startBob(sim);
    });
  }

  startBob(sim) {
    sim.seated = true;
    sim.seatedAt = Date.now();
    if (this.whiteboardText) this.refreshWhiteboard();
    this.tweens.add({
      targets: sim.label,
      alpha: 0.35,
      duration: 600,
      delay: 1200,
    });
    sim.bobTween = this.tweens.add({
      targets: sim.sprite,
      y: sim.sprite.y - 2,
      duration: 900,
      yoyo: true,
      repeat: -1,
      ease: "sine.inOut",
      onUpdate: () => {
        sim.label.x = sim.sprite.x;
        sim.label.y = sim.sprite.y + 32;
        if (sim.glyph) {
          sim.glyph.x = sim.sprite.x;
          sim.glyph.y = sim.sprite.y - 36;
        }
      },
    });
  }

  dismissSim(sim) {
    const wasInMeeting = sim.target && sim.target.kind === "meeting";
    sim.seated = false;
    sim.label.setAlpha(1);
    if (sim.bobTween) {
      sim.bobTween.stop();
      sim.bobTween = null;
    }
    if (wasInMeeting && this.whiteboardText) this.refreshWhiteboard();
    this.walkPath(sim, this.pathToDoorFrom(sim.target), () => {
      this.releaseTarget(sim);
      this.destroySim(sim);
      this.sims.delete(sim.id);
    });
  }

  walkPath(sim, waypoints, onDone) {
    const step = (i) => {
      if (i >= waypoints.length) {
        sim.walkTween = null;
        onDone?.();
        return;
      }
      const wp = waypoints[i];
      const dx = wp.x - sim.sprite.x;
      const dy = wp.y - sim.sprite.y;
      const dist = Math.hypot(dx, dy);
      const duration = Math.max(180, (dist / 110) * 1000);

      const legsTween = this.tweens.add({
        targets: sim.sprite.legs,
        y: 11.2,
        duration: 140,
        yoyo: true,
        repeat: Math.max(1, Math.floor(duration / 280)),
        ease: "sine.inOut",
      });
      sim.legsTween = legsTween;

      sim.walkTween = this.tweens.add({
        targets: sim.sprite,
        x: wp.x,
        y: wp.y,
        duration,
        ease: "sine.inOut",
        onUpdate: () => {
          sim.label.x = sim.sprite.x;
          sim.label.y = sim.sprite.y + 32;
          if (sim.glyph) {
            sim.glyph.x = sim.sprite.x;
            sim.glyph.y = sim.sprite.y - 36;
          }
          const now = performance.now();
          if (now - (sim.lastStepAt || 0) >= FOOTSTEP_INTERVAL_MS) {
            sim.lastStepAt = now;
            sim.footsteps.push({ x: sim.sprite.x, y: sim.sprite.y + 10, t: now });
            if (sim.footsteps.length > FOOTSTEP_MAX) sim.footsteps.shift();
          }
        },
        onComplete: () => {
          legsTween.stop();
          sim.legsTween = null;
          sim.sprite.legs.y = 12;
          step(i + 1);
        },
      });
    };
    step(0);
  }

  destroySim(sim) {
    if (sim.bobTween) sim.bobTween.stop();
    if (sim.walkTween) sim.walkTween.stop();
    if (sim.legsTween) sim.legsTween.stop();
    if (sim.haloG) {
      this.tweens.killTweensOf(sim.haloG);
      sim.haloG.destroy();
      sim.haloG = null;
    }
    sim.sprite.destroy();
    sim.label.destroy();
    if (sim.glyph) sim.glyph.destroy();
  }

  simGlyph(sim) {
    if (sim.errorUntil && performance.now() < sim.errorUntil) return "!";
    if (sim.visiting) return "🧪";
    if (sim.idle && sim.seated) return "💤";
    if (sim.walkTween) return "…";
    if (sim.seated && sim.kind === "plan") return "📋";
    if (sim.seated && sim.seatedAt && Date.now() - sim.seatedAt > GLYPH_IDLE_MS) return "Z";
    return "";
  }

  update() {
    const now = performance.now();
    const g = this.effects;
    if (!g) return;
    g.clear();

    // footstep trail — one pass across all sims, cheap per dot.
    for (const sim of this.sims.values()) {
      if (!sim.footsteps || sim.footsteps.length === 0) continue;
      while (sim.footsteps.length && now - sim.footsteps[0].t > FOOTSTEP_LIFETIME_MS) {
        sim.footsteps.shift();
      }
      for (const step of sim.footsteps) {
        const age = now - step.t;
        const alpha = 0.25 * Math.max(0, 1 - age / FOOTSTEP_LIFETIME_MS);
        if (alpha <= 0) continue;
        g.fillStyle(sim.shirtHex, alpha);
        g.fillCircle(step.x, step.y, 2);
      }
      if (sim.glyph) {
        const wanted = this.simGlyph(sim);
        if (sim.glyph.text !== wanted) sim.glyph.setText(wanted);
      }
    }

    // parent → child tether — dashed line from session sim to subagent, fading.
    if (this.tethers.length) {
      for (let i = this.tethers.length - 1; i >= 0; i--) {
        const t = this.tethers[i];
        const age = now - t.bornAt;
        if (age > TETHER_LIFETIME_MS) {
          this.tethers.splice(i, 1);
          continue;
        }
        const from = this.sims.get(t.fromId);
        const to = this.sims.get(t.toId);
        if (!from || !to) {
          this.tethers.splice(i, 1);
          continue;
        }
        const alpha = 0.4 * (1 - age / TETHER_LIFETIME_MS);
        const dx = to.sprite.x - from.sprite.x;
        const dy = to.sprite.y - from.sprite.y;
        const len = Math.hypot(dx, dy) || 1;
        const nx = dx / len;
        const ny = dy / len;
        const dash = 4;
        const gap = 3;
        g.lineStyle(1, to.shirtHex, alpha);
        for (let d = 0; d < len; d += dash + gap) {
          const x1 = from.sprite.x + nx * d;
          const y1 = from.sprite.y + ny * d;
          const x2 = from.sprite.x + nx * Math.min(d + dash, len);
          const y2 = from.sprite.y + ny * Math.min(d + dash, len);
          g.lineBetween(x1, y1, x2, y2);
        }
      }
    }

    // window-light breathing — map recent event rate to spill alpha.
    const cutoff = Date.now() - WINDOW_EVENT_WINDOW_MS;
    while (this.recentEvents.length && this.recentEvents[0] < cutoff) {
      this.recentEvents.shift();
    }
    const intensity = 1 - Math.exp(-this.recentEvents.length / 5);
    const targetAlpha =
      WINDOW_SPILL_BASE_ALPHA + (WINDOW_SPILL_PEAK_ALPHA - WINDOW_SPILL_BASE_ALPHA) * intensity;
    for (const spill of this.windowSpills) {
      spill.alpha += (targetAlpha - spill.alpha) * 0.08;
    }

    // tool-call motes — garbage-collect finished ones.
    if (this.motes.length) {
      for (let i = this.motes.length - 1; i >= 0; i--) {
        if (!this.motes[i].active) this.motes.splice(i, 1);
      }
    }

    // file-touch ticker alpha — peak 0.9 on a fresh entry, decay to 0.2 over 12 s.
    if (this.fileTickerText) {
      if (!this.fileTicker.length) {
        this.fileTickerText.setAlpha(0);
      } else {
        const newestAge = now - this.fileTicker[0].t;
        const k = Math.max(0, Math.min(1, newestAge / 12000));
        const a = 0.9 + (0.2 - 0.9) * k;
        this.fileTickerText.setAlpha(a);
      }
    }
  }
}

new Phaser.Game({
  type: Phaser.AUTO,
  parent: "game",
  width: WORLD_W,
  height: WORLD_H,
  backgroundColor: "#0b0d12",
  pixelArt: true,
  scale: {
    mode: Phaser.Scale.FIT,
    autoCenter: Phaser.Scale.CENTER_BOTH,
  },
  scene: [RoomScene],
});
