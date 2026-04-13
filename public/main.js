/* global Phaser */

const WORLD_W = 1024;
const WORLD_H = 640;

const ROOM = { x: 96, y: 72, w: 832, h: 512 };
const DOOR = { x: ROOM.x, y: ROOM.y + ROOM.h / 2, w: 10, h: 64 };
const OUTSIDE_X = 40;

const DESK_COLS = [260, 440, 620, 800];
const DESK_ROWS = [180, 340, 500];
const DESK_W = 96;
const DESK_H = 46;

const SEAT_OFFSET_Y = 44;
const APPROACH_OFFSET_Y = 76;

const PALETTE = {
  floorA: 0x3a2f24,
  floorB: 0x433627,
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

const QUEUE_SPOTS = [
  { x: ROOM.x + ROOM.w - 48, y: ROOM.y + 48 },
  { x: ROOM.x + ROOM.w - 48, y: ROOM.y + 108 },
  { x: ROOM.x + ROOM.w - 48, y: ROOM.y + ROOM.h - 100 },
  { x: ROOM.x + ROOM.w - 48, y: ROOM.y + ROOM.h - 40 },
];

function findFreeDesk() {
  return desks.find((d) => !d.taken) || null;
}

class RoomScene extends Phaser.Scene {
  constructor() {
    super("room");
    this.sims = new Map();
    this.queuedOverflow = 0;
  }

  create() {
    this.drawFloor();
    this.drawWalls();
    this.drawDecor();
    for (const d of desks) this.drawDesk(d);
    this.buildSimTextures();
    this.connect();
  }

  drawFloor() {
    const g = this.add.graphics();
    g.fillStyle(0x0b0d12, 1);
    g.fillRect(0, 0, WORLD_W, WORLD_H);

    const tile = 32;
    for (let y = ROOM.y; y < ROOM.y + ROOM.h; y += tile) {
      for (let x = ROOM.x; x < ROOM.x + ROOM.w; x += tile / 2) {
        const odd = (Math.floor(y / tile) + Math.floor(x / (tile / 2))) % 2;
        g.fillStyle(odd ? PALETTE.floorA : PALETTE.floorB, 1);
        g.fillRect(x, y, tile / 2, tile);
      }
    }

    g.lineStyle(1, PALETTE.floorLine, 0.4);
    for (let y = ROOM.y + tile; y < ROOM.y + ROOM.h; y += tile) {
      g.lineBetween(ROOM.x, y, ROOM.x + ROOM.w, y);
    }
  }

  drawWalls() {
    const g = this.add.graphics();
    const T = 6;
    g.fillStyle(PALETTE.wall, 1);
    g.fillRect(ROOM.x - T, ROOM.y - T, ROOM.w + 2 * T, T);
    g.fillRect(ROOM.x - T, ROOM.y + ROOM.h, ROOM.w + 2 * T, T);
    g.fillRect(ROOM.x - T, ROOM.y - T, T, ROOM.h + 2 * T);
    g.fillRect(ROOM.x + ROOM.w, ROOM.y - T, T, ROOM.h + 2 * T);

    g.fillStyle(PALETTE.wallHi, 1);
    g.fillRect(ROOM.x - T, ROOM.y - T, ROOM.w + 2 * T, 2);
    g.fillRect(ROOM.x - T, ROOM.y - T, 2, ROOM.h + 2 * T);

    g.fillStyle(0x0b0d12, 1);
    g.fillRect(ROOM.x - T, DOOR.y - DOOR.h / 2, T, DOOR.h);

    g.lineStyle(1, 0x6b84a8, 0.6);
    g.lineBetween(ROOM.x - T - 2, DOOR.y - DOOR.h / 2, ROOM.x - T - 2, DOOR.y + DOOR.h / 2);

    g.fillStyle(0xd6c9a5, 0.08);
    g.fillEllipse(ROOM.x + ROOM.w / 2, ROOM.y + ROOM.h / 2, ROOM.w * 0.8, ROOM.h * 0.9);
  }

  drawDecor() {
    const g = this.add.graphics();

    const potPositions = [
      { x: ROOM.x + 30, y: ROOM.y + 30 },
      { x: ROOM.x + ROOM.w - 30, y: ROOM.y + 30 },
      { x: ROOM.x + 30, y: ROOM.y + ROOM.h - 30 },
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

    // ground shadow under desk
    g.fillStyle(PALETTE.shadow, 0.28);
    g.fillEllipse(d.x, d.y + DESK_H / 2 + 4, DESK_W * 0.95, 8);

    // desk side (south face) — gives a hint of depth
    g.fillStyle(PALETTE.deskShade, 1);
    g.fillRoundedRect(x, y + DESK_H - 6, DESK_W, 6, 2);
    // desk top
    g.fillStyle(PALETTE.deskTop, 1);
    g.fillRoundedRect(x, y, DESK_W, DESK_H - 4, 2);
    g.lineStyle(1, PALETTE.deskEdge, 0.9);
    g.strokeRoundedRect(x, y, DESK_W, DESK_H - 4, 2);

    // wood grain — horizontal subtle lines
    g.lineStyle(1, PALETTE.deskEdge, 0.18);
    for (let py = y + 6; py < y + DESK_H - 8; py += 6) {
      g.lineBetween(x + 3, py, x + DESK_W - 3, py);
    }

    // monitor — sits on north half, faces south (screen visible to viewer)
    const mw = 34, mh = 16;
    const mx = d.x - mw / 2;
    const my = y + 3;
    // monitor stand (small trapezoid behind/below)
    g.fillStyle(0x2a2f39, 1);
    g.fillRect(d.x - 4, my + mh, 8, 3);
    g.fillRect(d.x - 8, my + mh + 3, 16, 2);
    // bezel
    g.fillStyle(0x1a1f28, 1);
    g.fillRoundedRect(mx - 1, my - 1, mw + 2, mh + 2, 2);
    // screen
    g.fillStyle(PALETTE.monitor, 1);
    g.fillRect(mx, my, mw, mh);
    g.fillStyle(PALETTE.monitorGlow, 0.8);
    g.fillRect(mx + 2, my + 2, mw - 4, mh - 4);
    // code-like lines on screen
    g.fillStyle(0xffffff, 0.45);
    g.fillRect(mx + 3, my + 3, 8, 1);
    g.fillRect(mx + 3, my + 5, 16, 1);
    g.fillRect(mx + 3, my + 7, 12, 1);
    g.fillRect(mx + 3, my + 9, 18, 1);
    g.fillRect(mx + 3, my + 11, 10, 1);
    g.fillRect(mx + 3, my + 13, 14, 1);

    // keyboard on south half
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

    // mouse to the right of keyboard
    g.fillStyle(PALETTE.mouse, 1);
    g.fillRoundedRect(d.x + 22, y + DESK_H - 13, 7, 5, 2);

    // coffee mug to the left
    g.fillStyle(0xffffff, 1);
    g.fillRoundedRect(x + 6, y + DESK_H - 16, 8, 9, 1);
    g.fillStyle(0x4a3221, 1);
    g.fillRect(x + 7, y + DESK_H - 15, 6, 2);
    g.lineStyle(1, 0xcfd6df, 1);
    g.strokeRect(x + 14, y + DESK_H - 14, 2, 4);

    // soft monitor glow ambient
    g.fillStyle(PALETTE.monitorGlow, 0.12);
    g.fillCircle(d.x, my + mh / 2, 36);

    // chair — sits south of the desk, north of the sim
    const chairY = d.y + DESK_H / 2 + 16;
    g.fillStyle(PALETTE.shadow, 0.3);
    g.fillEllipse(d.x, chairY + 6, 22, 5);
    // backrest (between desk and seat)
    g.fillStyle(PALETTE.chairHi, 1);
    g.fillRoundedRect(d.x - 12, chairY - 10, 24, 4, 1);
    // seat
    g.fillStyle(PALETTE.chair, 1);
    g.fillCircle(d.x, chairY, 9);
    g.lineStyle(1, PALETTE.chairHi, 1);
    g.strokeCircle(d.x, chairY, 9);
    g.fillStyle(PALETTE.chairHi, 1);
    g.fillCircle(d.x, chairY, 2);
  }

  buildSimTextures() {
    // Build a single "sim" composite sprite per unique user key on demand.
    // We use a Phaser texture atlas: body (tintable), hair (tintable), skin (base).
    // For simplicity, draw three textures then compose via 3 images.
    const g = this.make.graphics({ x: 0, y: 0, add: false });

    // body — 14x16 rounded torso; will be tinted
    g.clear();
    g.fillStyle(0xffffff, 1);
    g.fillRoundedRect(1, 3, 12, 12, 2);
    g.fillRect(0, 12, 14, 4);
    g.generateTexture("sim-body", 14, 16);

    // head — 10x10 head; tinted with skin color
    g.clear();
    g.fillStyle(0xffffff, 1);
    g.fillRoundedRect(0, 0, 10, 10, 3);
    g.generateTexture("sim-head", 10, 10);

    // hair — 10x6 top cap; tinted with pants color (reused as hair)
    g.clear();
    g.fillStyle(0xffffff, 1);
    g.fillRoundedRect(0, 0, 10, 6, 3);
    g.fillRect(0, 2, 10, 4);
    g.generateTexture("sim-hair", 10, 6);

    // legs — 12x5
    g.clear();
    g.fillStyle(0xffffff, 1);
    g.fillRect(1, 0, 4, 5);
    g.fillRect(7, 0, 4, 5);
    g.generateTexture("sim-legs", 12, 5);

    // shadow
    g.clear();
    g.fillStyle(0x000000, 1);
    g.fillEllipse(12, 4, 22, 7);
    g.generateTexture("sim-shadow", 24, 8);

    g.destroy();
  }

  makeSim(user) {
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
    if (msg.type === "snapshot") {
      for (const [id, sim] of this.sims) {
        this.destroySim(sim);
        this.sims.delete(id);
      }
      for (const a of msg.agents) this.spawnSim(a, { immediate: true });
    } else if (msg.type === "start") {
      this.spawnSim(msg.agent, { immediate: false });
    } else if (msg.type === "stop") {
      const sim = this.sims.get(msg.agent_id);
      if (sim) this.dismissSim(sim);
    }
  }

  spawnSim(agent, { immediate }) {
    if (this.sims.has(agent.agent_id)) return;

    const desk = findFreeDesk();
    let target;
    if (desk) {
      desk.taken = true;
      target = { kind: "desk", desk };
    } else {
      const spot = QUEUE_SPOTS[this.queuedOverflow++ % QUEUE_SPOTS.length];
      target = { kind: "queue", x: spot.x, y: spot.y };
    }

    const sprite = this.makeSim(agent.user || "unknown");
    sprite.setScale(1.8);

    const startX = immediate
      ? target.kind === "desk"
        ? target.desk.seatX
        : target.x
      : OUTSIDE_X;
    const startY = immediate
      ? target.kind === "desk"
        ? target.desk.seatY
        : target.y
      : DOOR.y;
    sprite.setPosition(startX, startY);

    const userTag = `${agent.user}@${truncate(agent.host || "", 14)}`;
    const taskTag = truncate(`${agent.agent_type} · ${agent.description || ""}`, 38);
    const label = this.add
      .text(startX, startY + 32, `${userTag}\n${taskTag}`, {
        fontFamily: "ui-monospace, Menlo, monospace",
        fontSize: "9px",
        color: "#f6f7f9",
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

    const sim = {
      id: agent.agent_id,
      sprite,
      label,
      target,
      bobTween: null,
      walkTween: null,
      seated: false,
    };
    this.sims.set(agent.agent_id, sim);

    if (immediate) this.startBob(sim);
    else this.walkIn(sim);
  }

  walkIn(sim) {
    const { target } = sim;
    const waypoints =
      target.kind === "desk"
        ? [
            { x: target.desk.approachX, y: target.desk.approachY },
            { x: target.desk.seatX, y: target.desk.seatY },
          ]
        : [{ x: target.x, y: target.y }];
    this.walkPath(sim, waypoints, () => {
      if (target.kind === "desk") this.startBob(sim);
    });
  }

  startBob(sim) {
    sim.seated = true;
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
      },
    });
  }

  dismissSim(sim) {
    sim.seated = false;
    sim.label.setAlpha(1);
    if (sim.bobTween) {
      sim.bobTween.stop();
      sim.bobTween = null;
    }
    const waypoints =
      sim.target.kind === "desk"
        ? [
            { x: sim.target.desk.approachX, y: sim.target.desk.approachY },
            { x: ROOM.x - 8, y: DOOR.y },
            { x: OUTSIDE_X, y: DOOR.y },
          ]
        : [{ x: ROOM.x - 8, y: DOOR.y }, { x: OUTSIDE_X, y: DOOR.y }];
    this.walkPath(sim, waypoints, () => {
      if (sim.target.kind === "desk") sim.target.desk.taken = false;
      this.destroySim(sim);
      this.sims.delete(sim.id);
    });
  }

  walkPath(sim, waypoints, onDone) {
    const step = (i) => {
      if (i >= waypoints.length) {
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

      sim.walkTween = this.tweens.add({
        targets: sim.sprite,
        x: wp.x,
        y: wp.y,
        duration,
        ease: "sine.inOut",
        onUpdate: () => {
          sim.label.x = sim.sprite.x;
          sim.label.y = sim.sprite.y + 32;
        },
        onComplete: () => {
          legsTween.stop();
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
    sim.sprite.destroy();
    sim.label.destroy();
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
