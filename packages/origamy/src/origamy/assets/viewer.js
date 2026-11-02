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

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";

const DURATION = 1100; // milliseconds a step's motion takes
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
    stage.show(step, animate);
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
    setTimeout(tick, DURATION + 500);
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
    stage.look("tilt", true, toward);
  }

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
    this.renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
    this.renderer.setPixelRatio(window.devicePixelRatio);
    element.append(this.renderer.domElement);
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color("#f6f5f2");
    this.scene.add(new THREE.HemisphereLight("#ffffff", "#c9c4ba", 2.2));
    const sun = new THREE.DirectionalLight("#ffffff", 1.4);
    sun.position.set(0.3, -0.5, 1);
    const below = new THREE.DirectionalLight("#ffffff", 0.9);
    below.position.set(-0.3, 0.5, -1);
    this.scene.add(sun, below);
    this.camera = new THREE.PerspectiveCamera(35, 1, 0.001, 100);
    this.camera.up.set(0, 0, 1);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
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
    this.seeThrough = false;
    this.lit = null;
    this.pointer = new THREE.Vector2();
    this.raycaster = new THREE.Raycaster();
    this.renderer.domElement.addEventListener("pointermove", (event) => this.hover(event));
    new ResizeObserver(() => this.resize()).observe(element);
    this.resize();
    this.look("tilt", true);
    const loop = (now) => {
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

  show(step, animate) {
    this.step = step;
    this.started = animate ? performance.now() : null;
    this.paper.clear();
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
    if (this.line) this.line.visible = t >= 1;
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

  animate(now) {
    if (this.started == null) return;
    const t = Math.min(1, (now - this.started) / DURATION);
    this.place(t);
    if (t >= 1) this.started = null;
  }

  look(view, instant = false, toward = null) {
    const box = new THREE.Box3().setFromObject(this.paper);
    const center = box.isEmpty() ? new THREE.Vector3(this.size / 2, this.size / 2, 0) : box.getCenter(new THREE.Vector3());
    // Shaped paper comes with an eye that faces it; flat paper is seen from in front.
    const offsets = {
      top: [0, -0.0001, 1],
      bottom: [0, -0.0001, -1],
      tilt: this.build.eye ?? [0.35, -0.75, 0.75],
    };
    const eye = new THREE.Vector3(...(toward ?? offsets[view])).normalize();
    // Back away until every corner of the paper, seen from there, fits the view both ways.
    const right = new THREE.Vector3(-eye.y, eye.x, 0);
    if (right.lengthSq() < 1e-6) right.set(1, 0, 0);
    right.normalize();
    const up = new THREE.Vector3().crossVectors(eye, right);
    const half = Math.tan((this.camera.fov * Math.PI) / 360);
    const corners = (this.faces ?? []).flatMap((face) => {
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
    this.controls.target.copy(center);
    this.camera.position.copy(center).addScaledVector(eye, distance);
    this.controls.update();
    if (instant) this.renderer.render(this.scene, this.camera);
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
