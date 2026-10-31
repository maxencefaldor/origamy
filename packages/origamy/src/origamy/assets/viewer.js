// Copyright 2026 Maxence Faldor
// The origamy viewer: replays a build step by step in 3D, beside its crease pattern.
//
// Each step holds every face's place before and after it, as 2D isometries. A face
// whose move turns it over makes a half turn about the move's axis, lifted toward the
// side it lands on; any other face glides. Layers sit a small gap apart, in stacking order.
// Once shaping turns the paper out of the plane, each face also has a rigid motion in
// space, and a step that turns paper in space rotates its moving faces about the turn's axis.

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";

const GAP = 0.0015; // height between layers, as a share of the paper's size
const DURATION = 1100; // milliseconds a step's motion takes
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

  pattern.onHover = (face) => stage.highlight(face);
  stage.onHover = (face) => pattern.highlight(face);
  go(steps.length - 1, false);
  return { go, stage, pattern, get index() { return index; } };
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
function motion(step, face, t, size, together) {
  const corners = step.faces[face].map((v) => step.vertices[v]);
  const before = step.before[face];
  const after = step.places[face];
  const z0 = step.orderBefore[face] * GAP * size;
  const z1 = step.order[face] * GAP * size;
  if (step.turn) return turning(step, face, corners, t, z0, z1);
  const shaped = step.motions?.[face] ?? STILL;
  return flat(step, face, corners, before, after, t, z0 + (z1 - z0) * t, together).map((p) => move(shaped, p));
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
    const up = together ?? step.order[face] >= step.orderBefore[face];
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
    this.place(animate ? 0 : 1);
    if (this.line) this.scene.remove(this.line);
    this.line = null;
    if (step.line) {
      const [[x0, y0], [x1, y1]] = step.line;
      const top = (Math.max(...step.order) + 2) * GAP * this.size;
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
    this.faces.forEach((face, index) => {
      const points = motion(step, index, eased, this.size, together);
      const positions = face.geometry.attributes.position;
      const outline = face.outline.attributes.position;
      points.forEach(([x, y, z], i) => {
        positions.setXYZ(i, x, y, z);
        outline.setXYZ(i, x, y, z + 0.0005 * this.size);
      });
      positions.needsUpdate = outline.needsUpdate = true;
      face.geometry.computeVertexNormals();
      face.geometry.computeBoundingSphere();
    });
    if (this.line) this.line.visible = t >= 1;
  }

  animate(now) {
    if (this.started == null) return;
    const t = Math.min(1, (now - this.started) / DURATION);
    this.place(t);
    if (t >= 1) this.started = null;
  }

  look(view, instant = false) {
    const box = new THREE.Box3().setFromObject(this.paper);
    const center = box.isEmpty() ? new THREE.Vector3(this.size / 2, this.size / 2, 0) : box.getCenter(new THREE.Vector3());
    const span = box.isEmpty() ? this.size : Math.max(box.max.x - box.min.x, box.max.y - box.min.y, this.size / 3);
    const distance = (span / 2 / Math.tan((this.camera.fov * Math.PI) / 360)) * 1.15;
    const offsets = {
      top: [0, -0.0001, 1],
      bottom: [0, -0.0001, -1],
      tilt: [0.35, -0.75, 0.75],
    };
    const [dx, dy, dz] = offsets[view];
    const length = Math.hypot(dx, dy, dz);
    this.controls.target.copy(center);
    this.camera.position.set(
      center.x + (dx / length) * distance,
      center.y + (dy / length) * distance,
      center.z + (dz / length) * distance,
    );
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
