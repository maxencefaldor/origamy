// Copyright 2026 Maxence Faldor
// The origamy viewer: replays a build step by step in 3D, beside its crease pattern.
//
// Paper bends where it folds: a crease whose two faces lie apart, as layers of a stack do,
// draws as a rounded bend between them, so a folded edge reads as one sheet turning round.
//
// Each step holds every face's place before and after it, as 2D isometries. A face
// whose move turns it over makes a half turn about the move's axis, lifted toward the
// side it lands on; any other face glides. Layers sit a small gap apart, each as high as the
// layers under it need.
// Once shaping turns the paper out of the plane, each face also has a rigid motion in
// space, and a step that turns paper in space rotates its moving faces about the turn's axis.
//
// The studio shows paper as paper: a soft room lights it, a key light casts its shadows on
// itself and on a table, and no outlines mark its faces.

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";

const DURATION = 1100; // milliseconds a step's motion takes
const GLIDE = 700; // milliseconds the eye takes to frame a film's next step, before the paper moves
const BEND = 8; // segments in the half turn a bend makes
const STROKES = {
  M: { color: "#c0392b", dash: "7 3 1.5 3", width: 1.6 },
  V: { color: "#2c6fbb", dash: "6 3", width: 1.6 },
  F: { color: "#9a9a9a", dash: "", width: 0.8 },
  B: { color: "#2b2b2b", dash: "", width: 1.6 },
};
const SVG = "http://www.w3.org/2000/svg";

export function start(root, build) {
  root.innerHTML = page(build);
  root.dataset.state = "ready";
  const stage = new Stage(root.querySelector(".stage"), build);
  const pattern = new Pattern(root.querySelector(".pattern"));
  const facts = root.querySelector(".facts");
  const label = root.querySelector(".label");
  const dots = root.querySelector(".dots");
  const steps = build.steps;
  let index = 0;
  let playing = false;

  steps.forEach((step, i) => {
    const dot = document.createElement("button");
    dot.className = "dot" + (step.refused ? " refused" : step.facts.length ? " warned" : "");
    dot.title = `${i}. ${step.description}`;
    dot.addEventListener("click", () => go(i, i === index + 1));
    dots.append(dot);
  });

  function go(target, animate) {
    index = Math.max(0, Math.min(steps.length - 1, target));
    const step = steps[index];
    stage.show(step, animate, index);
    pattern.show(step);
    label.innerHTML = describe(step, index, steps.length);
    facts.innerHTML = factsOf(step);
    dots.querySelectorAll(".dot").forEach((dot, i) => dot.classList.toggle("current", i === index));
    if (playing && index === steps.length - 1) toggle(false);
  }

  function toggle(on = !playing) {
    playing = on;
    root.querySelector('[data-do="play"]').textContent = playing ? "Pause" : "Play";
    if (playing) tick();
  }

  function tick() {
    if (!playing) return;
    if (index === steps.length - 1) go(0, false);
    else go(index + 1, true);
    setTimeout(tick, stage.duration() + 500);
  }

  const actions = {
    first: () => go(0, false),
    back: () => go(index - 1, false),
    play: () => toggle(),
    next: () => go(index + 1, true),
    last: () => go(steps.length - 1, false),
    top: () => stage.look("top"),
    bottom: () => stage.look("bottom"),
    tilt: () => stage.look("tilt"),
    xray: (button) => button.classList.toggle("on", stage.xray(!stage.seeThrough)),
    studio: (button) => button.classList.toggle("on", stage.studio(!stage.inStudio)),
  };
  root.querySelectorAll("[data-do]").forEach((button) =>
    button.addEventListener("click", () => actions[button.dataset.do](button)),
  );
  document.addEventListener("keydown", (event) => {
    const key = { ArrowRight: "next", ArrowLeft: "back", " ": "play", Home: "first", End: "last" }[event.key];
    if (key) {
      event.preventDefault();
      actions[key]();
    }
  });

  /** Show one step alone, filling the page, from a direction toward the eye. */
  function view(target, toward = null, seeThrough = false) {
    root.classList.add("bare");
    stage.resize();
    go(target, false);
    stage.xray(seeThrough);
    if (stage.line) stage.line.visible = false;
    stage.look("tilt", true, toward, true);
  }

  if (build.studio) actions.studio(root.querySelector('[data-do="studio"]'));
  pattern.onHover = (face) => stage.highlight(face);
  stage.onHover = (face) => pattern.highlight(face);
  go(steps.length - 1, false);
  return { go, view, stage, pattern, get index() { return index; } };
}

function page(build) {
  return `
<header><h1>${escape(build.name)}</h1><span>${build.steps.length - 1} steps · origamy</span></header>
<aside><figure class="pattern"></figure><section class="facts"></section></aside>
<main><div class="stage"></div>
  <nav class="views">
    <button data-do="top">Top</button><button data-do="tilt">3D</button>
    <button data-do="bottom">Bottom</button><button data-do="xray">X-ray</button>
    <button data-do="studio">Studio</button>
  </nav></main>
<footer>
  <div class="controls">
    <button data-do="first" title="First step (Home)">⏮</button>
    <button data-do="back" title="Previous step (←)">◀</button>
    <button data-do="play" title="Play (Space)">Play</button>
    <button data-do="next" title="Next step (→)">▶</button>
    <button data-do="last" title="Last step (End)">⏭</button>
  </div>
  <div class="label"></div>
  <div class="dots"></div>
</footer>`;
}

function describe(step, index, count) {
  const where = step.source ? `<code>${escape(step.source)}</code>` : "";
  const head = step.refused ? "Refused" : `Step ${index} of ${count - 1}`;
  return `<strong>${head}</strong> ${escape(step.description)} ${where}`;
}

function factsOf(step) {
  if (!step.facts.length) return `<p class="quiet">No facts to state about this step.</p>`;
  return step.facts
    .map(
      (fact) => `<p class="fact ${fact.level}"><b>${fact.level}[${escape(fact.rule)}]</b> ` +
        `${escape(fact.message)}${fact.faces.length ? ` <span>faces ${fact.faces.join(", ")}</span>` : ""}</p>`,
    )
    .join("");
}

function escape(text) {
  return String(text).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

// 2D isometries as [xx, xy, yx, yy, tx, ty].
const apply = (m, [x, y]) => [m[0] * x + m[1] * y + m[4], m[2] * x + m[3] * y + m[5]];
const inverse = (m) => [m[0], m[2], m[1], m[3], -(m[0] * m[4] + m[2] * m[5]), -(m[1] * m[4] + m[3] * m[5])];
const compose = (a, b) => [
  a[0] * b[0] + a[1] * b[2], a[0] * b[1] + a[1] * b[3],
  a[2] * b[0] + a[3] * b[2], a[2] * b[1] + a[3] * b[3],
  a[0] * b[4] + a[1] * b[5] + a[4], a[2] * b[4] + a[3] * b[5] + a[5],
];

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const unit = (v) => {
  const length = Math.hypot(...v) || 1;
  return v.map((c) => c / length);
};
const mean = (points) => [0, 1, 2].map((k) => points.reduce((s, p) => s + p[k], 0) / points.length);
/** The way the front of a face looks, from its corners, counterclockwise on the crease pattern. */
const normal = (points) => unit(cross(points[1].map((c, k) => c - points[0][k]), points[2].map((c, k) => c - points[0][k])));

// Rigid motions of space as a 3 by 3 rotation, row by row, then a shift.
const STILL = [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0];
const move = (m, [x, y, z]) => [
  m[0] * x + m[1] * y + m[2] * z + m[9],
  m[3] * x + m[4] * y + m[5] * z + m[10],
  m[6] * x + m[7] * y + m[8] * z + m[11],
];

/** A share of a turn about an axis, as a rigid motion. */
function rotation({ point, direction: [x, y, z], angle }, share) {
  const [c, s] = [Math.cos(angle * share), Math.sin(angle * share)];
  const k = 1 - c;
  const r = [
    k * x * x + c, k * x * y - s * z, k * x * z + s * y,
    k * x * y + s * z, k * y * y + c, k * y * z - s * x,
    k * x * z - s * y, k * y * z + s * x, k * z * z + c,
  ];
  const moved = move([...r, 0, 0, 0], point);
  return [...r, point[0] - moved[0], point[1] - moved[1], point[2] - moved[2]];
}

/** Where a face's corners are, in 3D, at a share `t` of its move. */
function motion(step, face, t, gap, together) {
  const corners = step.faces[face].map((v) => step.vertices[v]);
  const before = step.before[face];
  const after = step.places[face];
  const z0 = step.heightBefore[face] * gap;
  const z1 = step.height[face] * gap;
  // A stack turned out of the plane shifts to meet the paper it turned from.
  const [s0, s1] = [step.shiftBefore[face], step.shift[face]];
  const shift = s0.map((s, k) => (s + (s1[k] - s) * t) * gap);
  const shaped = step.path.length ? along(step, face, t) : step.motions?.[face] ?? STILL;
  // A path holds the paper whole as its creases turn together: faces ride its forms.
  const points = step.turn
    ? turning(step, face, corners, t, z0, z1)
    : step.path.length
      ? corners.map((p) => move(shaped, [...apply(after, p), z0 + (z1 - z0) * t]))
      : flat(step, face, corners, before, after, t, z0 + (z1 - z0) * t, together).map((p) => move(shaped, p));
  return points.map((p) => p.map((c, k) => c + shift[k]));
}

/** Where a flex has moved a face at a share `t` of it, between the forms it passes. */
function along(step, face, t) {
  const forms = [step.motionsBefore?.[face] ?? STILL, ...step.path.map((m) => m[face]), step.motions?.[face] ?? STILL];
  const at = t * (forms.length - 1);
  const k = Math.min(forms.length - 2, Math.floor(at));
  return blend(forms[k], forms[k + 1], at - k);
}

/** A share of the way from one rigid motion to another: slerped, then slid. */
function blend(a, b, t) {
  const matrix = (m) => new THREE.Matrix4().set(m[0], m[1], m[2], m[9], m[3], m[4], m[5], m[10], m[6], m[7], m[8], m[11], 0, 0, 0, 1);
  const [pa, pb] = [new THREE.Vector3(), new THREE.Vector3()];
  const [qa, qb] = [new THREE.Quaternion(), new THREE.Quaternion()];
  const scale = new THREE.Vector3();
  matrix(a).decompose(pa, qa, scale);
  matrix(b).decompose(pb, qb, scale);
  const e = new THREE.Matrix4().compose(pa.lerp(pb, t), qa.slerp(qb, t), new THREE.Vector3(1, 1, 1)).elements;
  return [e[0], e[4], e[8], e[1], e[5], e[9], e[2], e[6], e[10], e[12], e[13], e[14]];
}

/** A face of paper that turns in space: about the turn's axis when it moves. */
function turning(step, face, corners, t, z0, z1) {
  const lift = (place, z, shaped) => corners.map(([x, y]) => move(shaped, [...apply(place, [x, y]), z]));
  const start = lift(step.before[face], z0, step.motionsBefore?.[face] ?? STILL);
  const end = lift(step.places[face], z1, step.motions[face]);
  if (!step.moved.includes(face)) return start.map((p, i) => p.map((c, k) => c + (end[i][k] - c) * t));
  const [now, whole] = [rotation(step.turn, t), rotation(step.turn, 1)];
  // The turn takes the face to its end, but for the gap between layers, which closes as it goes.
  return start.map((p, i) => {
    const [turned, landed] = [move(now, p), move(whole, p)];
    return turned.map((c, k) => c + (end[i][k] - landed[k]) * t);
  });
}

/** A face of paper that lies flat, at height `z`. */
function flat(step, face, corners, before, after, t, z, together) {
  const delta = compose(after, inverse(before));
  const starts = corners.map((p) => apply(before, p));
  const [a, b, c, d, tx, ty] = delta;
  if (a * d - b * c < 0) {
    // A turn over: a half turn about the reflection's axis, plus any glide along it.
    const phi = Math.atan2(c, a) / 2;
    const u = [Math.cos(phi), Math.sin(phi)];
    const n = [-u[1], u[0]];
    const along = tx * u[0] + ty * u[1];
    const offset = (tx * n[0] + ty * n[1]) / 2;
    const distances = starts.map(([x, y]) => x * n[0] + y * n[1] - offset);
    const mean = distances.reduce((s, v) => s + v, 0) / distances.length;
    const up = together ?? step.height[face] >= step.heightBefore[face];
    const sense = together !== null ? 1 : (up ? 1 : -1) * Math.sign(mean || 1);
    const angle = Math.PI * t;
    return starts.map(([x, y], i) => {
      const dist = distances[i];
      const back = dist * (1 - Math.cos(angle));
      return [
        x - back * n[0] + along * t * u[0],
        y - back * n[1] + along * t * u[1],
        z + sense * dist * Math.sin(angle),
      ];
    });
  }
  // A glide: turn about the rotation's center, or slide when nothing turns.
  const turn = Math.atan2(c, a) * t;
  if (Math.abs(Math.atan2(c, a)) < 1e-9) {
    return starts.map(([x, y]) => [x + tx * t, y + ty * t, z]);
  }
  const det = (1 - a) * (1 - d) - b * c;
  const cx = ((1 - d) * tx + b * ty) / det;
  const cy = (c * tx + (1 - a) * ty) / det;
  const [cos, sin] = [Math.cos(turn), Math.sin(turn)];
  return starts.map(([x, y]) => [
    cx + cos * (x - cx) - sin * (y - cy),
    cy + sin * (x - cx) + cos * (y - cy),
    z,
  ]);
}

class Stage {
  constructor(element, build) {
    this.element = element;
    this.build = build;
    this.size = build.size;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
    this.renderer.setPixelRatio(window.devicePixelRatio);
    this.renderer.shadowMap.type = THREE.VSMShadowMap;
    element.append(this.renderer.domElement);
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color("#f6f5f2");
    const sky = new THREE.HemisphereLight("#ffffff", "#c9c4ba", 2.2);
    const sun = new THREE.DirectionalLight("#ffffff", 1.4);
    sun.position.set(0.3, -0.5, 1);
    const below = new THREE.DirectionalLight("#ffffff", 0.9);
    below.position.set(-0.3, 0.5, -1);
    // The studio's key light casts the paper's shadows, on itself and on the table.
    const key = new THREE.DirectionalLight("#fffaf2", 0);
    key.castShadow = true;
    key.shadow.mapSize.set(1024, 1024);
    key.shadow.radius = 6;
    key.shadow.blurSamples = 8;
    this.lights = { sky, sun, below, key };
    this.scene.add(sky, sun, below, key, key.target);
    this.table = new THREE.Mesh(new THREE.PlaneGeometry(1, 1), new THREE.ShadowMaterial({ opacity: 0.16 }));
    this.table.receiveShadow = true;
    this.table.visible = false;
    this.scene.add(this.table);
    this.inStudio = false;
    // The depth buffer spans the paper's scale, so layers a breath apart still draw in order.
    this.camera = new THREE.PerspectiveCamera(35, 1, this.size / 50, this.size * 50);
    this.camera.up.set(0, 0, 1);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    // Once a hand turns the view, the eye stays where the hand left it.
    this.held = false;
    this.controls.addEventListener("start", () => {
      this.held = true;
      this.glide = null;
    });
    this.paper = new THREE.Group();
    this.scene.add(this.paper);
    this.line = null;
    this.materials = {
      front: new THREE.MeshStandardMaterial({ color: build.paper.front, side: THREE.FrontSide, roughness: 0.9, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 }),
      back: new THREE.MeshStandardMaterial({ color: build.paper.back, side: THREE.BackSide, roughness: 0.9, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 }),
      edge: new THREE.LineBasicMaterial({ color: "#3a3a3a" }),
      lit: new THREE.LineBasicMaterial({ color: "#e0182d" }),
      fold: new THREE.LineDashedMaterial({ color: "#2c6fbb", dashSize: 0.02, gapSize: 0.014 }),
      refused: new THREE.LineDashedMaterial({ color: "#e0182d", dashSize: 0.02, gapSize: 0.014 }),
    };
    // A simulated build plays its frames in place of rigid faces.
    this.film = build.film ? new Film(this.scene, build.film, this.materials, build.size) : null;
    this.seeThrough = false;
    this.lit = null;
    this.pointer = new THREE.Vector2();
    this.raycaster = new THREE.Raycaster();
    this.renderer.domElement.addEventListener("pointermove", (event) => this.hover(event));
    new ResizeObserver(() => this.resize()).observe(element);
    this.resize();
    this.look("tilt", true);
    const loop = (now) => {
      this.steer(now);
      this.animate(now);
      this.controls.update();
      this.renderer.render(this.scene, this.camera);
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
  }

  resize() {
    const { clientWidth: width, clientHeight: height } = this.element;
    if (!width || !height) return;
    this.renderer.setSize(width, height);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
  }

  show(step, animate, index = 0) {
    this.step = step;
    this.started = animate ? performance.now() : null;
    this.paper.clear();
    if (this.film) {
      // The simulated paper stands in for the rigid faces, and no fold line shows.
      this.faces = [];
      this.bends = [];
      if (this.line) this.scene.remove(this.line);
      this.line = null;
      // The eye frames each step before its paper moves, and holds still while it folds.
      this.film.step = this.film.steps[index] ?? this.film.steps.at(-1);
      const glide = animate && !this.held && this.aim(this.view ?? "tilt");
      this.film.show(index, animate, glide ? GLIDE : 0);
      if (glide) this.glide = { ...glide, started: performance.now() };
      else if (!this.held) this.look(this.view ?? "tilt", true);
      this.studio(this.inStudio);
      this.place(animate ? 0 : 1);
      return;
    }
    this.faces = step.faces.map((corners, face) => {
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.Float32BufferAttribute(new Float32Array(corners.length * 3), 3));
      geometry.setIndex(corners.slice(2).flatMap((_, i) => [0, i + 1, i + 2]));
      const outline = new THREE.BufferGeometry();
      outline.setAttribute("position", new THREE.Float32BufferAttribute(new Float32Array(corners.length * 3), 3));
      const group = new THREE.Group();
      const front = new THREE.Mesh(geometry, this.materials.front);
      const back = new THREE.Mesh(geometry, this.materials.back);
      const edge = new THREE.LineLoop(outline, this.materials.edge);
      front.userData.face = back.userData.face = face;
      group.add(front, back, edge);
      this.paper.add(group);
      return { geometry, outline, edge, front, back };
    });
    const strip = Array.from({ length: BEND }, (_, k) => [2 * k, 2 * k + 1, 2 * k + 3, 2 * k, 2 * k + 3, 2 * k + 2]).flat();
    this.bends = step.creases.map(([u, w, f, g]) => {
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.Float32BufferAttribute(new Float32Array((BEND + 1) * 6), 3));
      geometry.setIndex(strip);
      const front = new THREE.Mesh(geometry, this.materials.front);
      const back = new THREE.Mesh(geometry, this.materials.back);
      front.userData.face = back.userData.face = f;
      this.paper.add(front, back);
      const corners = [step.faces[f].indexOf(u), step.faces[f].indexOf(w), step.faces[g].indexOf(u), step.faces[g].indexOf(w)];
      return { geometry, front, back, f, g, corners };
    });
    this.studio(this.inStudio);
    this.place(animate ? 0 : 1);
    if (this.line) this.scene.remove(this.line);
    this.line = null;
    if (step.line) {
      const [[x0, y0], [x1, y1]] = step.line;
      const top = (Math.max(...step.height) + 2) * this.build.gap * this.size;
      let ends = [[x0, y0, top], [x1, y1, top]];
      if (step.turn) {
        // A turn in space draws its axis, as long as the line across the paper.
        const { point: p, direction: d } = step.turn;
        const half = Math.hypot(x1 - x0, y1 - y0) / 2;
        ends = [-1, 1].map((s) => p.map((c, k) => c + s * half * d[k]));
      }
      const geometry = new THREE.BufferGeometry().setFromPoints(ends.map((e) => new THREE.Vector3(...e)));
      this.line = new THREE.Line(geometry, step.refused ? this.materials.refused : this.materials.fold);
      this.line.computeLineDistances();
      this.scene.add(this.line);
    }
    if (!this.framed) {
      this.framed = true;
      this.look("tilt", true);
    }
  }

  place(t) {
    if (this.film) return this.film.place(t);
    const step = this.step;
    // When every face moves, the whole paper turns as one.
    const together = step.moved.length === step.faces.length ? true : null;
    const eased = t < 1 ? 0.5 - Math.cos(Math.PI * t) / 2 : 1;
    const placed = this.faces.map((face, index) => {
      const points = motion(step, index, eased, this.build.gap * this.size, together);
      const positions = face.geometry.attributes.position;
      const outline = face.outline.attributes.position;
      points.forEach(([x, y, z], i) => {
        positions.setXYZ(i, x, y, z);
        outline.setXYZ(i, x, y, z + 0.0005 * this.size);
      });
      positions.needsUpdate = outline.needsUpdate = true;
      face.geometry.computeVertexNormals();
      face.geometry.computeBoundingSphere();
      return points;
    });
    for (const bend of this.bends) this.bendAt(bend, placed);
    // The studio shows the paper alone, without the step's fold line.
    if (this.line) this.line.visible = t >= 1 && !this.inStudio;
    if (this.inStudio) this.light();
  }

  /** Show paper as paper, or as a diagram: lit by a soft room, casting shadows, without outlines. */
  studio(on) {
    this.inStudio = on;
    const { sky, sun, below, key } = this.lights;
    this.renderer.shadowMap.enabled = on;
    this.renderer.toneMapping = on ? THREE.NeutralToneMapping : THREE.NoToneMapping;
    this.renderer.toneMappingExposure = 1;
    if (on && !this.room) this.room = new THREE.PMREMGenerator(this.renderer).fromScene(new RoomEnvironment(), 0.04).texture;
    this.scene.environment = on ? this.room : null;
    this.scene.background = on ? null : new THREE.Color("#f6f5f2");
    sky.intensity = on ? 0.45 : 2.2;
    sun.intensity = on ? 0 : 1.4;
    below.intensity = on ? 0.15 : 0.9;
    key.intensity = on ? 1.05 : 0;
    this.table.visible = on;
    for (const material of [this.materials.front, this.materials.back]) {
      material.envMapIntensity = 0.25;
      material.needsUpdate = true;
    }
    for (const face of this.faces ?? []) {
      face.edge.visible = !on;
      for (const mesh of [face.front, face.back]) mesh.castShadow = mesh.receiveShadow = on;
    }
    for (const bend of this.bends ?? []) {
      for (const mesh of [bend.front, bend.back]) mesh.castShadow = mesh.receiveShadow = on;
    }
    this.film?.shadows(on);
    this.element.classList.toggle("studio", on);
    if (this.line) this.line.visible = !on && !this.started;
    if (on && this.faces) this.light();
    return on;
  }

  /** Aim the key light's shadows at the paper, and lay the table under it. */
  light() {
    // A film lights every place its paper reaches, so the light and table stay still as it plays.
    // Otherwise the box reads the paper's vertices, as the faces' cached bounds lag their moves.
    const box = this.film ? new THREE.Box3().setFromPoints(this.film.reach) : new THREE.Box3().setFromObject(this.paper, true);
    if (box.isEmpty()) return;
    const center = box.getCenter(new THREE.Vector3());
    const reach = box.getSize(new THREE.Vector3()).length() / 2 + 1e-6;
    const { key } = this.lights;
    key.target.position.copy(center);
    key.position.copy(center).add(new THREE.Vector3(-0.8, -0.45, 0.75).normalize().multiplyScalar(6 * reach));
    // The shadow camera sees the whole table, so no edge of its view crosses the table.
    const camera = key.shadow.camera;
    camera.left = camera.bottom = -3 * reach;
    camera.right = camera.top = 3 * reach;
    camera.near = 2 * reach;
    camera.far = 10 * reach;
    camera.updateProjectionMatrix();
    key.shadow.bias = -0.0001;
    this.table.position.set(center.x, center.y, box.min.z - 1e-4 * reach);
    this.table.scale.setScalar(4 * reach);
  }

  /** Draw a crease as the half turn its paper makes from one face to the other. */
  bendAt(bend, placed) {
    const [P, Q] = [placed[bend.f], placed[bend.g]];
    const [fu, fw, gu, gw] = bend.corners;
    const ends = [[P[fu], Q[gu]], [P[fw], Q[gw]]];
    const apart = Math.max(...ends.map(([a, b]) => Math.hypot(...b.map((c, k) => c - a[k]))));
    const shown = apart > 1e-6 * this.size;
    bend.front.visible = bend.back.visible = shown;
    if (!shown) return;
    const along = unit(P[fw].map((c, k) => c - P[fu][k]));
    // Each face lies inward of the crease; the paper bends round on the side away from both.
    const inward = (points, a, b) => {
      const middle = a.map((c, k) => (c + b[k]) / 2);
      const toward = mean(points).map((c, k) => c - middle[k]);
      return unit(toward.map((c, k) => c - dot(toward, along) * along[k]));
    };
    const [inF, inG] = [inward(P, P[fu], P[fw]), inward(Q, Q[gu], Q[gw])];
    const outward = inF.map((c, k) => -(c + inG[k]));
    const curved = Math.hypot(...outward) > 0.5;
    const positions = bend.geometry.attributes.position;
    ends.forEach(([a, b], end) => {
      const rise = b.map((c, k) => c - a[k]);
      const radius = Math.hypot(...rise) / 2;
      const up = unit(rise);
      const out = unit(outward.map((c, k) => c - dot(outward, up) * up[k]));
      for (let i = 0; i <= BEND; i++) {
        const angle = (Math.PI * i) / BEND;
        const point = curved
          ? a.map((c, k) => (c + b[k]) / 2 - radius * Math.cos(angle) * up[k] + radius * Math.sin(angle) * out[k])
          : a.map((c, k) => c + (rise[k] * i) / BEND);
        positions.setXYZ(2 * i + end, ...point);
      }
    });
    positions.needsUpdate = true;
    bend.geometry.computeVertexNormals();
    bend.geometry.computeBoundingSphere();
    // The bend shows the paper's front on the side face f shows it.
    const start = curved ? unit(outward) : unit(ends[0][1].map((c, k) => c - ends[0][0][k]));
    const facing = dot(cross(along, start), normal(P));
    const [front, back] = facing >= 0 ? [this.materials.front, this.materials.back] : [this.materials.back, this.materials.front];
    bend.front.material = front;
    bend.back.material = back;
  }

  /** How long the step on show takes to move, in milliseconds. */
  duration() {
    return this.film ? this.film.duration : DURATION;
  }

  animate(now) {
    if (this.started == null) return;
    const t = Math.min(1, (now - this.started) / this.duration());
    this.place(t);
    if (t >= 1) this.started = null;
  }

  /**
   * Move the eye to frame the paper from a view, for good unless a hand turns it again.
   *
   * With `settled`, a film's eye frames only where the step leaves its paper.
   */
  look(view, instant = false, toward = null, settled = false) {
    this.view = view;
    this.held = false;
    this.glide = null;
    const aim = this.aim(view, toward, settled);
    if (aim) {
      this.controls.target.copy(aim.target);
      this.camera.position.copy(aim.position);
      this.controls.update();
    }
    if (instant) this.renderer.render(this.scene, this.camera);
  }

  /** Glide the eye toward where it frames the film's step. */
  steer(now) {
    if (!this.glide) return;
    const s = Math.min(1, (now - this.glide.started) / GLIDE);
    const eased = 0.5 - Math.cos(Math.PI * s) / 2;
    this.camera.position.lerpVectors(this.glide.from.position, this.glide.position, eased);
    this.controls.target.lerpVectors(this.glide.from.target, this.glide.target, eased);
    if (s >= 1) this.glide = null;
  }

  /**
   * Where the eye goes to frame the paper from a view, and where it looks.
   *
   * A film's eye frames the step on show: where its paper rests before and after, and
   * where it passes between; or, `settled`, where it rests after alone. Returns null for
   * a film whose eye is there already.
   */
  aim(view, toward = null, settled = false) {
    const step = this.film?.step;
    const rests = step && (settled ? step.settled : step.rests);
    const box = step ? new THREE.Box3().setFromPoints(rests) : new THREE.Box3().setFromObject(this.paper);
    const center = box.isEmpty() ? new THREE.Vector3(this.size / 2, this.size / 2, 0) : box.getCenter(new THREE.Vector3());
    // Shaped paper comes with an eye that faces it; flat paper is seen from in front.
    const flat = [0.35, -0.75, 0.75];
    const standing = step ? step.standing : true;
    const offsets = {
      top: [0, -0.0001, 1],
      bottom: [0, -0.0001, -1],
      tilt: standing ? (this.build.eye ?? flat) : flat,
    };
    const eye = new THREE.Vector3(...(toward ?? offsets[view])).normalize();
    // Back away until every corner of the paper, seen from there, fits the view both ways.
    const right = new THREE.Vector3(-eye.y, eye.x, 0);
    if (right.lengthSq() < 1e-6) right.set(1, 0, 0);
    right.normalize();
    const up = new THREE.Vector3().crossVectors(eye, right);
    const half = Math.tan((this.camera.fov * Math.PI) / 360);
    const corners = step ? rests : (this.faces ?? []).flatMap((face) => {
      const positions = face.geometry.attributes.position;
      return Array.from({ length: positions.count }, (_, i) => new THREE.Vector3().fromBufferAttribute(positions, i));
    });
    // Center the paper as the eye sees it, across and up.
    for (const axis of [right, up]) {
      const reach = corners.map((c) => c.clone().sub(center).dot(axis));
      if (reach.length) center.addScaledVector(axis, (Math.min(...reach) + Math.max(...reach)) / 2);
    }
    // A corner nearer the eye needs the eye as much farther back as it lies nearer.
    let distance = (this.size / 6 / half) * 1.1;
    for (const corner of corners) {
      const offset = corner.clone().sub(center);
      const fit = Math.max(Math.abs(offset.dot(right)) / (half * this.camera.aspect), Math.abs(offset.dot(up)) / half);
      distance = Math.max(distance, fit * 1.1 + offset.dot(eye));
    }
    // Paper passing between rests may come near the edge of the view, but stays in it.
    for (const corner of (!settled && step?.reach) || []) {
      const offset = corner.clone().sub(center);
      const fit = Math.max(Math.abs(offset.dot(right)) / (half * this.camera.aspect), Math.abs(offset.dot(up)) / half);
      distance = Math.max(distance, fit * 1.01 + offset.dot(eye));
    }
    const position = center.clone().addScaledVector(eye, distance);
    const from = { position: this.camera.position.clone(), target: this.controls.target.clone() };
    const there = from.position.distanceTo(position) + from.target.distanceTo(center) < 1e-3 * this.size;
    if (step && there) return null;
    return { position, target: center, from };
  }

  xray(on) {
    this.seeThrough = on;
    for (const material of [this.materials.front, this.materials.back]) {
      material.transparent = on;
      material.opacity = on ? 0.32 : 1;
      material.depthWrite = !on;
      material.needsUpdate = true;
    }
    return on;
  }

  hover(event) {
    const rect = this.renderer.domElement.getBoundingClientRect();
    this.pointer.set(((event.clientX - rect.left) / rect.width) * 2 - 1, -((event.clientY - rect.top) / rect.height) * 2 + 1);
    this.raycaster.setFromCamera(this.pointer, this.camera);
    const hit = this.raycaster.intersectObjects(this.paper.children, true).find((h) => h.object.userData.face !== undefined);
    const face = hit ? hit.object.userData.face : null;
    if (face !== this.lit) {
      this.highlight(face);
      this.onHover?.(face);
    }
  }

  highlight(face) {
    if (this.lit !== null && this.faces?.[this.lit]) this.faces[this.lit].edge.material = this.materials.edge;
    this.lit = face;
    if (face !== null && this.faces?.[face]) this.faces[face].edge.material = this.materials.lit;
  }
}

// --- The film: a build as origamy's simulator folded it. ---

const FILM_STEP = 40; // milliseconds a simulated increment takes
const FILM_MOVE = 900; // milliseconds a rigid move of the whole paper takes
const FILM_LAYER = 1 / 5000; // touching layers' distance as drawn, in paper sizes: about a sheet of kami
const FILM_MOVED = 1 / 150; // the least move, in paper sizes, that may be a rigid move of the whole paper
const FILM_RIGID = 1 / 750; // the farthest, in paper sizes, a rigid move may miss the paper

const FILM_GRAIN = 3; // times the paper's fibers repeat across the sheet

/**
 * Paper's fibers, as two tileable pictures: one tints the paper's color, one bumps it.
 *
 * The tint mottles the color by a percent or two, in soft clouds. The bumps are short
 * strands, raised or sunk, that catch the light as paper's fibers do.
 */
function fibers() {
  const size = 512;
  // A seeded random, so every viewer draws the same paper.
  let seed = 7;
  const random = () => ((seed = (seed * 16807) % 2147483647) - 1) / 2147483646;
  const canvas = (fill) => {
    const element = document.createElement("canvas");
    element.width = element.height = size;
    const context = element.getContext("2d");
    context.fillStyle = fill;
    context.fillRect(0, 0, size, size);
    return [element, context];
  };
  // Draw at every place a tile repeats, so the picture wraps without a seam.
  const around = (draw) => {
    for (const dx of [-size, 0, size]) for (const dy of [-size, 0, size]) draw(dx, dy);
  };
  const [tint, shade] = canvas("#f6f6f6");
  for (let k = 0; k < 60; k++) {
    const [x, y, r, light] = [random() * size, random() * size, 40 + random() * 90, random() < 0.5 ? 236 : 255];
    around((dx, dy) => {
      const cloud = shade.createRadialGradient(x + dx, y + dy, 0, x + dx, y + dy, r);
      cloud.addColorStop(0, `rgba(${light},${light},${light},0.18)`);
      cloud.addColorStop(1, `rgba(${light},${light},${light},0)`);
      shade.fillStyle = cloud;
      shade.fillRect(x + dx - r, y + dy - r, 2 * r, 2 * r);
    });
  }
  const [bumps, strands] = canvas("#808080");
  strands.lineCap = "round";
  for (let k = 0; k < 2600; k++) {
    const [x, y, angle, length] = [random() * size, random() * size, random() * Math.PI, 3 + random() * 14];
    const level = random() < 0.5 ? 100 : 156;
    strands.strokeStyle = `rgba(${level},${level},${level},${0.25 + random() * 0.35})`;
    strands.lineWidth = 0.6 + random() * 0.9;
    const bend = (random() - 0.5) * 1.2;
    around((dx, dy) => {
      strands.beginPath();
      strands.moveTo(x + dx, y + dy);
      strands.quadraticCurveTo(
        x + dx + Math.cos(angle + bend) * length * 0.5,
        y + dy + Math.sin(angle + bend) * length * 0.5,
        x + dx + Math.cos(angle) * length,
        y + dy + Math.sin(angle) * length,
      );
      strands.stroke();
    });
  }
  return [tint, bumps].map((element, k) => {
    const texture = new THREE.CanvasTexture(element);
    texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
    texture.colorSpace = k === 0 ? THREE.SRGBColorSpace : THREE.NoColorSpace;
    texture.anisotropy = 8;
    return texture;
  });
}

function decode(text, Type) {
  const raw = atob(text);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return new Type(bytes.buffer);
}

/** The eigenvector of a symmetric 4 by 4 matrix's largest eigenvalue, by Jacobi's rotations. */
function topEigenvector(matrix) {
  const a = matrix.map((row) => row.slice());
  const v = [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]];
  for (let sweep = 0; sweep < 50; sweep++) {
    let off = 0;
    for (let p = 0; p < 4; p++) for (let q = p + 1; q < 4; q++) off += a[p][q] ** 2;
    if (off < 1e-30) break;
    for (let p = 0; p < 4; p++) {
      for (let q = p + 1; q < 4; q++) {
        if (Math.abs(a[p][q]) < 1e-300) continue;
        const theta = (a[q][q] - a[p][p]) / (2 * a[p][q]);
        const t = Math.sign(theta || 1) / (Math.abs(theta) + Math.sqrt(theta * theta + 1));
        const c = 1 / Math.sqrt(t * t + 1);
        const s = t * c;
        for (let k = 0; k < 4; k++) [a[k][p], a[k][q]] = [c * a[k][p] - s * a[k][q], s * a[k][p] + c * a[k][q]];
        for (let k = 0; k < 4; k++) [a[p][k], a[q][k]] = [c * a[p][k] - s * a[q][k], s * a[p][k] + c * a[q][k]];
        for (let k = 0; k < 4; k++) [v[k][p], v[k][q]] = [c * v[k][p] - s * v[k][q], s * v[k][p] + c * v[k][q]];
      }
    }
  }
  let best = 0;
  for (let k = 1; k < 4; k++) if (a[k][k] > a[best][best]) best = k;
  return [v[0][best], v[1][best], v[2][best], v[3][best]];
}

/** The rigid motion that best takes points `a` to points `b`, by Horn's method, and how far it misses. */
function rigidFit(a, b, n) {
  const [ca, cb] = [[0, 0, 0], [0, 0, 0]];
  for (let i = 0; i < n; i++) for (let k = 0; k < 3; k++) {
    ca[k] += a[3 * i + k] / n;
    cb[k] += b[3 * i + k] / n;
  }
  const m = [[0, 0, 0], [0, 0, 0], [0, 0, 0]];
  for (let i = 0; i < n; i++) for (let r = 0; r < 3; r++) for (let c = 0; c < 3; c++) {
    m[r][c] += (a[3 * i + r] - ca[r]) * (b[3 * i + c] - cb[c]);
  }
  const [[xx, xy, xz], [yx, yy, yz], [zx, zy, zz]] = m;
  const q = topEigenvector([
    [xx + yy + zz, yz - zy, zx - xz, xy - yx],
    [yz - zy, xx - yy - zz, xy + yx, zx + xz],
    [zx - xz, xy + yx, -xx + yy - zz, yz + zy],
    [xy - yx, zx + xz, yz + zy, -xx - yy + zz],
  ]);
  const rotation = new THREE.Quaternion(q[1], q[2], q[3], q[0]).normalize();
  const point = new THREE.Vector3();
  let miss = 0;
  for (let i = 0; i < n; i++) {
    point.set(a[3 * i] - ca[0], a[3 * i + 1] - ca[1], a[3 * i + 2] - ca[2]).applyQuaternion(rotation);
    miss = Math.max(miss, Math.hypot(point.x + cb[0] - b[3 * i], point.y + cb[1] - b[3 * i + 1], point.z + cb[2] - b[3 * i + 2]));
  }
  return { rotation, from: ca, to: cb, miss };
}

class Film {
  constructor(scene, film, materials, size) {
    const n = (this.n = film.vertices);
    this.size = size;
    this.triangles = decode(film.triangles, Int32Array);
    const faces = decode(film.faces, Int32Array);
    const rest = decode(film.rest, Float32Array);
    const steps = film.steps.map((step) => ({
      folds: step.folds.map((fold) => ({ ...fold, lifts: decode(fold.lifts, Uint8Array) })),
      frames: step.frames ? decode(step.frames, Float32Array) : new Float32Array(0),
    }));
    // The paper lies on the table: each frame comes down to where the flat sheet lay, as a
    // hand lowers paper it lifted to fold under.
    const first = steps.find((step) => step.frames.length).frames;
    let lying = Infinity;
    for (let i = 0; i < n; i++) lying = Math.min(lying, first[3 * i + 2]);
    for (const step of steps) {
      for (let f = 0; f < step.frames.length / (3 * n); f++) {
        const frame = step.frames.subarray(3 * n * f, 3 * n * (f + 1));
        let low = Infinity;
        for (let i = 0; i < n; i++) low = Math.min(low, frame[3 * i + 2]);
        for (let i = 0; i < n; i++) frame[3 * i + 2] += lying - low;
      }
    }
    const flat = new Float32Array(3 * n);
    for (let i = 0; i < n; i++) flat.set([rest[2 * i], rest[2 * i + 1], lying], 3 * i);
    // Between steps a hand slides the paper back to the middle of the table, where the sheet lay.
    const middle = (positions) => {
      let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
      for (let i = 0; i < n; i++) {
        [x0, x1] = [Math.min(x0, positions[3 * i]), Math.max(x1, positions[3 * i])];
        [y0, y1] = [Math.min(y0, positions[3 * i + 1]), Math.max(y1, positions[3 * i + 1])];
      }
      return [(x0 + x1) / 2, (y0 + y1) / 2];
    };
    const home = middle(flat);
    for (const step of steps) {
      const count = step.frames.length / (3 * n);
      if (!count) continue;
      const [x, y] = middle(step.frames.subarray(3 * n * (count - 1)));
      step.shift = [home[0] - x, home[1] - y];
      for (let f = 0; f < count; f++) {
        for (let i = 0; i < n; i++) {
          step.frames[3 * n * f + 3 * i] += step.shift[0];
          step.frames[3 * n * f + 3 * i + 1] += step.shift[1];
        }
      }
    }
    const faceCount = Math.max(...faces) + 1;
    const still = { lifts: new Uint8Array(3 * this.triangles.length), flips: new Array(faceCount).fill(false) };
    // Each step ends where its last frame lies, or where the step before it ended.
    this.steps = [];
    let end = { positions: flat, fold: still };
    let shift = [0, 0];
    for (const step of steps) {
      const shots = [];
      if (step.shift) {
        const [dx, dy] = [step.shift[0] - shift[0], step.shift[1] - shift[1]];
        if (Math.hypot(dx, dy) > FILM_MOVED * size) {
          const slid = Float32Array.from(end.positions);
          for (let i = 0; i < n; i++) {
            slid[3 * i] += dx;
            slid[3 * i + 1] += dy;
          }
          shots.push({ positions: slid, fold: end.fold });
        }
        shift = step.shift;
      }
      step.folds.forEach((fold, k) => {
        const before = step.folds.slice(0, k).reduce((sum, f) => sum + f.count, 0);
        for (let f = 0; f < fold.count; f++) {
          shots.push({ positions: step.frames.subarray(3 * n * (before + f), 3 * n * (before + f + 1)), fold });
        }
      });
      this.steps.push({ start: end, shots });
      if (shots.length) end = shots[shots.length - 1];
      this.steps[this.steps.length - 1].end = end;
    }
    // The eye frames where the paper rests between steps, and keeps in view where it passes
    // during them. Each keeps a point once to a cell of a fine grid.
    const kept = (shots) => {
      const cell = size / 64;
      const cells = new Map();
      for (const { positions: p } of shots) {
        for (let i = 0; i < n; i++) {
          const key = `${Math.round(p[3 * i] / cell)},${Math.round(p[3 * i + 1] / cell)},${Math.round(p[3 * i + 2] / cell)}`;
          if (!cells.has(key)) cells.set(key, new THREE.Vector3(p[3 * i], p[3 * i + 1], p[3 * i + 2]));
        }
      }
      return [...cells.values()];
    };
    this.reach = kept(this.steps.flatMap((step) => [step.start, ...step.shots]));
    for (const step of this.steps) {
      step.rests = kept([step.start, step.end]);
      step.settled = kept([step.end]);
      step.reach = kept(step.shots);
      // Paper that stands up from the table needs the eye that faces the finished model.
      let [low, high] = [Infinity, -Infinity];
      for (let i = 0; i < n; i++) [low, high] = [Math.min(low, step.end.positions[3 * i + 2]), Math.max(high, step.end.positions[3 * i + 2])];
      step.standing = high - low > size / 12;
    }
    // Each face of the last crease pattern draws as its own surface, with its own vertices.
    this.group = new THREE.Group();
    scene.add(this.group);
    this.groups = [];
    for (let face = 0; face < faceCount; face++) {
      const own = [];
      for (let t = 0; t < faces.length; t++) if (faces[t] === face) own.push(t);
      if (!own.length) continue;
      const local = new Map();
      const vertices = [];
      const corners = new Int32Array(3 * own.length);
      own.forEach((t, k) => {
        for (let c = 0; c < 3; c++) {
          const v = this.triangles[3 * t + c];
          if (!local.has(v)) {
            local.set(v, vertices.length);
            vertices.push(v);
          }
          corners[3 * k + c] = local.get(v);
        }
      });
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array(3 * vertices.length), 3));
      geometry.setAttribute("normal", new THREE.BufferAttribute(new Float32Array(3 * vertices.length), 3));
      // The fibers stay with the paper, wherever it goes.
      const uv = new Float32Array(2 * vertices.length);
      vertices.forEach((v, i) => uv.set([(rest[2 * v] / size) * FILM_GRAIN, (rest[2 * v + 1] / size) * FILM_GRAIN], 2 * i));
      geometry.setAttribute("uv", new THREE.BufferAttribute(uv, 2));
      geometry.setIndex(Array.from(corners));
      const meshes = [materials.front, materials.back].map((material) => new THREE.Mesh(geometry, material));
      this.group.add(...meshes);
      const offset = this.groups.length ? this.groups.at(-1).offset + this.groups.at(-1).vertices.length : 0;
      this.groups.push({ face, own, vertices, corners, geometry, meshes, local, offset });
    }
    // Paper's edges catch a line of shade: its raw edges, and its creases as far as they fold.
    const sides = new Map();
    for (let t = 0; t < this.triangles.length / 3; t++) {
      for (let c = 0; c < 3; c++) {
        const [u, v] = [this.triangles[3 * t + c], this.triangles[3 * t + ((c + 1) % 3)]];
        const key = u < v ? `${u},${v}` : `${v},${u}`;
        if (!sides.has(key)) sides.set(key, []);
        sides.get(key).push(t);
      }
    }
    const groupOf = new Map(this.groups.map((group) => [group.face, group]));
    this.edges = [];
    for (const [key, triangles] of sides) {
      const [u, v] = key.split(",").map(Number);
      const kinds = triangles.map((t) => faces[t]);
      if (triangles.length === 2 && kinds[0] === kinds[1]) continue;
      triangles.forEach((t, k) => {
        const group = groupOf.get(kinds[k]);
        const ends = [group.offset + group.local.get(u), group.offset + group.local.get(v)];
        this.edges.push({ group, u, v, ends, triangle: t, other: triangles.length === 2 ? triangles[1 - k] : -1 });
      });
    }
    const lines = new THREE.BufferGeometry();
    lines.setAttribute("position", new THREE.BufferAttribute(new Float32Array(6 * this.edges.length), 3));
    lines.setAttribute("color", new THREE.BufferAttribute(new Float32Array(8 * this.edges.length), 4));
    this.lines = new THREE.LineSegments(lines, new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, depthWrite: false }));
    this.lines.frustumCulled = false;
    this.group.add(this.lines);
    const [tint, bumps] = fibers();
    for (const material of [materials.front, materials.back]) {
      material.map = tint;
      material.bumpMap = bumps;
      material.bumpScale = 0.35;
      material.needsUpdate = true;
    }
    this.positions = new Float32Array(3 * n);
    this.duration = DURATION;
  }

  shadows(on) {
    for (const group of this.groups) for (const mesh of group.meshes) mesh.castShadow = on;
  }

  /** Get ready to show a step: its passages from where the step before left the paper. */
  show(index, animate, lead = 0) {
    const step = this.steps[index] ?? this.steps[this.steps.length - 1];
    this.step = step;
    const shots = [step.start, ...step.shots];
    this.passages = [];
    let clock = lead;
    for (let k = 1; k < shots.length; k++) {
      const [a, b] = [shots[k - 1].positions, shots[k].positions];
      let moved = 0;
      for (let i = 0; i < 3 * this.n; i++) moved = Math.max(moved, Math.abs(a[i] - b[i]));
      const motion = moved > FILM_MOVED * this.size ? rigidFit(a, b, this.n) : null;
      const rigid = motion !== null && motion.miss < FILM_RIGID * this.size;
      const span = rigid ? FILM_MOVE : FILM_STEP;
      this.passages.push({ a, b, rigid, motion, start: clock, span, fold: shots[k].fold });
      clock += span;
    }
    this.duration = Math.max(clock, 1);
    if (!animate) this.place(1);
  }

  /** Lay the paper where it lies a share `t` of the way through the step. */
  place(t) {
    const step = this.step;
    if (!this.passages.length) return this.draw(step.end.positions, step.end.fold);
    const time = t * this.duration;
    const passage = this.passages.find((p) => time < p.start + p.span) ?? this.passages[this.passages.length - 1];
    const s = Math.min(1, Math.max(0, (time - passage.start) / passage.span));
    const out = this.positions;
    const { a, b } = passage;
    if (passage.rigid) {
      const eased = 0.5 - Math.cos(Math.PI * s) / 2;
      const turn = new THREE.Quaternion().slerp(passage.motion.rotation, eased);
      const { from, to } = passage.motion;
      const point = new THREE.Vector3();
      for (let i = 0; i < this.n; i++) {
        point.set(a[3 * i] - from[0], a[3 * i + 1] - from[1], a[3 * i + 2] - from[2]).applyQuaternion(turn);
        out[3 * i] = point.x + from[0] + (to[0] - from[0]) * eased;
        out[3 * i + 1] = point.y + from[1] + (to[1] - from[1]) * eased;
        out[3 * i + 2] = point.z + from[2] + (to[2] - from[2]) * eased;
      }
    } else {
      for (let i = 0; i < 3 * this.n; i++) out[i] = a[i] + (b[i] - a[i]) * s;
    }
    this.draw(out, passage.fold);
  }

  /** Draw the paper at positions, each vertex lifted a sheet for each layer below it there. */
  draw(positions, fold) {
    for (const group of this.groups) {
      const sheet = FILM_LAYER * this.size * (fold.flips[group.face] ? -1 : 1);
      const sums = new Float32Array(3 * group.vertices.length);
      group.own.forEach((t, k) => {
        const [a, b, c] = [0, 1, 2].map((j) => this.triangles[3 * t + j]);
        const ab = [0, 1, 2].map((j) => positions[3 * b + j] - positions[3 * a + j]);
        const ac = [0, 1, 2].map((j) => positions[3 * c + j] - positions[3 * a + j]);
        const normal = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
        for (let j = 0; j < 3; j++) {
          const v = group.corners[3 * k + j];
          for (let d = 0; d < 3; d++) sums[3 * v + d] += normal[d];
        }
      });
      const position = group.geometry.attributes.position;
      const normal = group.geometry.attributes.normal;
      group.vertices.forEach((v, i) => {
        const length = Math.hypot(sums[3 * i], sums[3 * i + 1], sums[3 * i + 2]) || 1;
        const [x, y, z] = [sums[3 * i] / length, sums[3 * i + 1] / length, sums[3 * i + 2] / length];
        const lift = fold.lifts[group.offset + i] * sheet;
        position.setXYZ(i, positions[3 * v] + lift * x, positions[3 * v + 1] + lift * y, positions[3 * v + 2] + lift * z);
        normal.setXYZ(i, x, y, z);
      });
      position.needsUpdate = normal.needsUpdate = true;
      group.geometry.computeBoundingSphere();
      group.geometry.computeBoundingBox();
    }
    this.shade(positions, fold);
  }

  /** Draw the paper's edges half a layer over their faces, as dark as they fold. */
  shade(positions, fold) {
    const normalOf = (t) => {
      const [a, b, c] = [0, 1, 2].map((j) => this.triangles[3 * t + j]);
      const ab = [0, 1, 2].map((j) => positions[3 * b + j] - positions[3 * a + j]);
      const ac = [0, 1, 2].map((j) => positions[3 * c + j] - positions[3 * a + j]);
      const cross = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
      const length = Math.hypot(...cross) || 1;
      return cross.map((x) => x / length);
    };
    const position = this.lines.geometry.attributes.position;
    const color = this.lines.geometry.attributes.color;
    this.edges.forEach((edge, k) => {
      const sheet = FILM_LAYER * this.size * (fold.flips[edge.group.face] ? -1 : 1);
      const normal = normalOf(edge.triangle);
      let alpha = 0.3;
      if (edge.other >= 0) {
        const other = normalOf(edge.other);
        const bent = 1 - (normal[0] * other[0] + normal[1] * other[1] + normal[2] * other[2]);
        alpha = 0.05 + 0.25 * Math.min(1, bent);
      }
      [edge.u, edge.v].forEach((v, end) => {
        const lift = (fold.lifts[edge.ends[end]] + 0.5) * sheet;
        position.setXYZ(2 * k + end, positions[3 * v] + lift * normal[0], positions[3 * v + 1] + lift * normal[1], positions[3 * v + 2] + lift * normal[2]);
        color.setXYZW(2 * k + end, 0.16, 0.13, 0.1, alpha);
      });
    });
    position.needsUpdate = color.needsUpdate = true;
  }
}

class Pattern {
  constructor(element) {
    this.element = element;
    this.lit = null;
  }

  show(step) {
    const xs = step.vertices.map(([x]) => x);
    const ys = step.vertices.map(([, y]) => y);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const span = Math.max(x1 - x0, y1 - y0);
    const pad = span * 0.04;
    const svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("viewBox", `${x0 - pad} ${-y1 - pad} ${x1 - x0 + 2 * pad} ${y1 - y0 + 2 * pad}`);
    const scale = span / 300;
    const partial = step.moved.length < step.faces.length;
    this.polygons = step.faces.map((corners, face) => {
      const polygon = document.createElementNS(SVG, "polygon");
      polygon.setAttribute("points", corners.map((v) => `${step.vertices[v][0]},${-step.vertices[v][1]}`).join(" "));
      polygon.setAttribute("class", partial && step.moved.includes(face) ? "face moved" : "face");
      polygon.addEventListener("pointerenter", () => {
        this.highlight(face);
        this.onHover?.(face);
      });
      polygon.addEventListener("pointerleave", () => {
        this.highlight(null);
        this.onHover?.(null);
      });
      svg.append(polygon);
      return polygon;
    });
    for (const [u, v, kind] of [...step.edges].sort((a, b) => (a[2] === "B") - (b[2] === "B"))) {
      const stroke = STROKES[kind];
      const line = document.createElementNS(SVG, "line");
      const [[ax, ay], [bx, by]] = [step.vertices[u], step.vertices[v]];
      Object.entries({ x1: ax, y1: -ay, x2: bx, y2: -by, stroke: stroke.color, "stroke-width": stroke.width * scale }).forEach(
        ([key, value]) => line.setAttribute(key, value),
      );
      if (stroke.dash) line.setAttribute("stroke-dasharray", stroke.dash.split(" ").map((d) => d * scale).join(" "));
      line.setAttribute("pointer-events", "none");
      svg.append(line);
    }
    this.element.replaceChildren(svg);
    const caption = document.createElement("figcaption");
    caption.innerHTML = `Crease pattern · <span class="m">mountain</span> <span class="v">valley</span> <span class="f">flat</span>`;
    this.element.append(caption);
  }

  highlight(face) {
    this.polygons?.[this.lit]?.classList.remove("lit");
    this.lit = face;
    if (face !== null) this.polygons?.[face]?.classList.add("lit");
  }
}
