import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { OutputPass } from "three/examples/jsm/postprocessing/OutputPass.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { SMAAPass } from "three/examples/jsm/postprocessing/SMAAPass.js";
import { UnrealBloomPass } from "three/examples/jsm/postprocessing/UnrealBloomPass.js";
import { call } from "../../lib/api";
import Icon from "../../components/Icons";
import Select from "../../components/Select";
import { installAnimatedMaterials, type AnimatedMaterials } from "./render/animatedMaterials";
import { attachNprOutlines } from "./render/nprOutline";
import { createPreviewClip, type PreviewAnimation } from "./render/previewAnimation";
import { assignAbilitySlots, type PreviewAnimationChoice } from "./render/previewActions";
import { BG_COLOR, DEFAULT_CFG, LIGHTS, loadCfg, saveCapture, storeCfg, type Bg, type LightPreset, type ViewerCfg } from "./viewerConfig";

type Props = { hero: string; codes: string[]; addon: string | null };

// decoded models are heavy: only the last few are kept so switching back and forth is quick
const cache = new Map<string, ArrayBuffer>();
const KEEP = 4;

async function fetchModel(hero: string, codes: string[], addon: string | null): Promise<ArrayBuffer> {
  const key = `${hero}|${codes[0]}|${addon ?? ""}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const raw = await call<ArrayBuffer | number[]>("hero_model", { hero, codes, addon });
  const buf = raw instanceof ArrayBuffer ? raw : new Uint8Array(raw).buffer;
  cache.set(key, buf);
  while (cache.size > KEEP) cache.delete(cache.keys().next().value as string);
  return buf;
}

function disposeTree(root: THREE.Object3D) {
  root.traverse((o) => {
    const m = o as THREE.Mesh;
    if (!m.isMesh) return;
    m.geometry.dispose();
    for (const mat of Array.isArray(m.material) ? m.material : [m.material]) {
      for (const v of Object.values(mat)) if (v instanceof THREE.Texture) v.dispose();
      mat.dispose();
    }
  });
}

type ShowResult = { groups: BodyGroup[] };
type ViewerApi = {
  /** Null when a newer model was requested while this one was still being prepared. */
  show: (buf: ArrayBuffer, keepView?: boolean) => Promise<ShowResult | null>;
  reset: () => void;
  choose: (chosen: Record<string, number>) => void;
  configure: (c: ViewerCfg) => void;
  snap: (x: number, y: number) => void;
  shot: () => Promise<Blob | null>;
  stream: () => MediaStream;
  /** Plays decoded bone tracks on the model; null returns it to its rest pose. */
  playTracks: (animation: PreviewAnimation | null) => void;
  setPlaying: (on: boolean) => void;
  seek: (t: number) => void;
  info: () => { time: number; dur: number };
};

const AXES = [
  { v: [1, 0, 0], color: "#e0674f", label: "X" },
  { v: [0, 1, 0], color: "#e6c35c", label: "Y" },
  { v: [0, 0, 1], color: "#6c9fd8", label: "Z" },
] as const;
const GIZMO = 96;

type Ability = { class: string; name: string; icon: string | null; clip: string | null };

const RU: Record<string, string> = {
  idle: "покой", melee: "ближний бой", dash: "рывок", parry: "парирование", reload: "перезарядка", shoot: "стрельба", cast: "каст",
  generic: "общий", ability: "способность", start: "начало", loop: "цикл", end: "конец", fire: "выстрел", release: "отпускание",
  hold: "удержание", draw: "натяжение", intro: "начало", outro: "конец", recover: "восстановление", channeling: "накачка",
  jump: "прыжок", slide: "скольжение", crouch: "приседание", stand: "стойка", hit: "удар", quick: "быстрый", air: "в воздухе",
  inair: "в воздухе", ground: "на земле", throw: "бросок", lift: "подъём", grab: "захват", knockdown: "нокдаун", large: "большой",
  medium: "средний", small: "малый", getup: "подъём", ui: "меню", hero: "герой", reveal: "появление", select: "выбор", pose: "поза",
  emote: "эмоция", sleep: "сон", shopping: "магазин", landing: "приземление", impact: "удар", item: "предмет", weapon: "оружие",
  charged: "заряженный", shot: "выстрел", guided: "управляемая", arrow: "стрела", power: "мощный", missile: "ракета", channel: "канал",
  run: "бег", stop: "остановка", turn: "поворот", left: "влево", right: "вправо", mantle: "подтягивание", death: "смерть",
  dive: "пикирование", hover: "парение", leap: "прыжок", precast: "подготовка", toss: "бросок", charge: "рывок",
};
/** "ability_charged_shot" as a readable label; Russian words where the dictionary has them. */
const clipLabel = (n: string, ru: boolean) =>
  n
    .split("_")
    .map((w) => (ru ? (RU[w] ?? w) : w))
    .join(" ");

const clock2 = (s: number) => Math.floor(s / 60) + ":" + (s % 60).toFixed(2).padStart(5, "0");

type Choice = { name: string; bit: number };
type BodyGroup = { name: string; choices: Choice[] };

/**
 * Switchable body groups of the model (a skin can ship a second form, such as a transformed ultimate).
 * The game shows one choice per group; a viewer that shows every mesh draws all forms at once.
 * Choices without meshes are left out: skins often keep the table but drop the alternates.
 */
function readGroups(root: THREE.Object3D): { groups: BodyGroup[]; chosen: Record<string, number> } {
  const meta = root.userData as { meshGroups?: { bodyGroup: string; name: string; bit: number }[]; defaultMeshGroups?: number[] };
  const drawn = new Set<number>();
  root.traverse((o) => {
    const bits = (o as THREE.Mesh).isMesh ? ((o as THREE.Mesh).geometry.userData as { meshGroups?: number[] }).meshGroups : undefined;
    for (const b of bits ?? []) drawn.add(b);
  });
  const byName = new Map<string, BodyGroup>();
  for (const c of meta.meshGroups ?? []) {
    if (!drawn.has(c.bit)) continue;
    const g = byName.get(c.bodyGroup) ?? { name: c.bodyGroup, choices: [] };
    g.choices.push({ name: c.name, bit: c.bit });
    byName.set(c.bodyGroup, g);
  }
  const shown = new Set(meta.defaultMeshGroups ?? []);
  const groups = [...byName.values()].filter((g) => g.choices.length > 1);
  const chosen: Record<string, number> = {};
  for (const g of groups) chosen[g.name] = (g.choices.find((c) => shown.has(c.bit)) ?? g.choices[0]).bit;
  return { groups, chosen };
}

/** Shows the meshes of the chosen choices and hides the other choices of the same groups. */
function applyGroups(root: THREE.Object3D, groups: BodyGroup[], chosen: Record<string, number>) {
  const switchable = new Set(groups.flatMap((g) => g.choices.map((c) => c.bit)));
  const shown = new Set(Object.values(chosen));
  root.traverse((o) => {
    const m = o as THREE.Mesh;
    if (!m.isMesh) return;
    const bits = (m.geometry.userData as { meshGroups?: number[] }).meshGroups;
    if (!bits || !bits.some((b) => switchable.has(b))) return;
    m.visible = bits.some((b) => shown.has(b));
  });
}

/** A model viewer: drag to orbit, wheel to zoom, right button to pan. */
export default function HeroViewer({ hero, codes, addon }: Props) {
  const { t, i18n } = useTranslation();
  const ru = i18n.language.startsWith("ru");
  const host = useRef<HTMLDivElement>(null);
  const api = useRef<ViewerApi | null>(null);
  const view = useRef<HTMLDivElement>(null);
  const gizmo = useRef<HTMLCanvasElement>(null);
  const [cfg, setCfg] = useState<ViewerCfg>(loadCfg);
  const cfgRef = useRef(cfg);
  cfgRef.current = cfg;
  const [menu, setMenu] = useState<"view" | "studio" | null>(null);
  const [playing, setPlaying] = useState(true);
  const [pos, setPos] = useState({ time: 0, dur: 0 });
  const [full, setFull] = useState(false);
  const [rec, setRec] = useState<{ at: number } | null>(null);
  const [secs, setSecs] = useState(0);
  const recorder = useRef<MediaRecorder | null>(null);
  const [note, setNote] = useState("");
  // what the hero can be shown doing (movements and abilities made of the game's clips); `actionId` is the one
  // playing, null for the rest pose
  const [choices, setChoices] = useState<PreviewAnimationChoice[]>([]);
  const [actionId, setActionId] = useState<string | null>(null);
  const [animBusy, setAnimBusy] = useState(false);
  // bumped each time a model has been put in the scene, so the animation is applied to the model that is there
  const [shownTick, setShownTick] = useState(0);
  const [abilities, setAbilities] = useState<Ability[]>([]);
  // the same abilities in English: clip names are matched against these
  const [abilitiesEn, setAbilitiesEn] = useState<Ability[]>([]);
  const [soft, setSoft] = useState(false);
  const loaded = useRef<{ hero: string; addon: string | null } | null>(null);
  const [groups, setGroups] = useState<BodyGroup[]>([]);
  const [chosen, setChosen] = useState<Record<string, number>>({});
  const table = useRef<BodyGroup[]>([]);
  const [showVariants, setShowVariants] = useState(false);
  const [state, setState] = useState<"loading" | "ready" | "error">("loading");
  const [error, setError] = useState("");
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    // A filmic curve and strong lights give glossy parts the highlights they have in the game; the final
    // OutputPass applies this curve to the post-processed frame.
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 0.95;
    el.appendChild(renderer.domElement);
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x09090b);
    const camera = new THREE.PerspectiveCamera(35, 1, 0.01, 1000);
    // Image-based light from a neutral studio: surfaces keep the colours of their textures
    // (plain coloured lamps tint pale skin) and glossy parts have something to reflect.
    const pmrem = new THREE.PMREMGenerator(renderer);
    const room = new RoomEnvironment();
    scene.environment = pmrem.fromScene(room, 0.04).texture;
    scene.environmentIntensity = 1;
    const hemi = new THREE.HemisphereLight(0xe9eef9, 0x242022, 0.35);
    scene.add(hemi);
    const key = new THREE.DirectionalLight(0xfff1dc, 1.1);
    key.position.set(2.8, 4.2, 3.6);
    scene.add(key);
    const rim = new THREE.DirectionalLight(0xff4965, 0.6);
    rim.position.set(-3.5, 2.4, -2.4);
    scene.add(rim);
    // The scene is drawn in HDR (with a stencil buffer for the outlines) and blooms only what is brighter than
    // white: self-lit material, whose glTF emissive strength runs past 1. Tone mapping happens once, in the output pass.
    const composer = new EffectComposer(renderer, new THREE.WebGLRenderTarget(1, 1, { type: THREE.HalfFloatType, stencilBuffer: true }));
    composer.addPass(new RenderPass(scene, camera));
    const bloom = new UnrealBloomPass(new THREE.Vector2(1, 1), 0.3, 0.3, 1.2);
    composer.addPass(bloom);
    composer.addPass(new OutputPass());
    // the composer's targets have no multisampling of their own, so edges are smoothed as a last pass
    const smaa = new SMAAPass(1, 1);
    composer.addPass(smaa);
    let grid: THREE.GridHelper | null = null;
    let restPose: { bone: THREE.Bone; position: THREE.Vector3; quaternion: THREE.Quaternion; scale: THREE.Vector3 }[] = [];
    let action: THREE.AnimationAction | null = null;
    let live: ViewerCfg = cfgRef.current;
    let hits: { x: number; y: number; dir: THREE.Vector3 }[] = [];
    // the game's own material animation (scrolling, jitter, translucency, self-illumination) and the toon outline
    let animated: AnimatedMaterials | null = null;
    let outlines: ReturnType<typeof attachNprOutlines> | null = null;
    let simTime = 0;
    let running = true;
    let showTicket = 0;
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    let model: THREE.Object3D | null = null;
    let mixer: THREE.AnimationMixer | null = null;
    const clock = new THREE.Clock();
    let home = { pos: new THREE.Vector3(0, 1, 4), target: new THREE.Vector3(0, 1, 0) };

    const resize = () => {
      const w = el.clientWidth || 1;
      const h = el.clientHeight || 1;
      renderer.setSize(w, h);
      composer.setPixelRatio(renderer.getPixelRatio());
      composer.setSize(w, h);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      outlines?.setViewportHeight(h);
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const drawGizmo = () => {
      const cv = gizmo.current;
      const ctx = cv?.getContext("2d");
      if (!cv || !ctx) return;
      if (cv.width !== GIZMO * dpr) {
        cv.width = GIZMO * dpr;
        cv.height = GIZMO * dpr;
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, GIZMO, GIZMO);
      const inv = camera.quaternion.clone().invert();
      const c = GIZMO / 2;
      const items = AXES.flatMap((a) =>
        [1, -1].map((sign) => {
          const dir = new THREE.Vector3(...a.v).multiplyScalar(sign);
          const p = dir.clone().applyQuaternion(inv);
          return { a, sign, dir, x: c + p.x * 30, y: c - p.y * 30, z: p.z };
        }),
      );
      items.sort((l, r) => l.z - r.z);
      hits = items.map((i) => ({ x: i.x, y: i.y, dir: i.dir }));
      ctx.font = "600 10px system-ui, sans-serif";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      for (const i of items) {
        if (i.sign > 0) {
          ctx.strokeStyle = i.a.color;
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.moveTo(c, c);
          ctx.lineTo(i.x, i.y);
          ctx.stroke();
          ctx.fillStyle = i.a.color;
          ctx.beginPath();
          ctx.arc(i.x, i.y, 9, 0, Math.PI * 2);
          ctx.fill();
          ctx.fillStyle = "#14161c";
          ctx.fillText(i.a.label, i.x, i.y + 0.5);
        } else {
          ctx.fillStyle = i.a.color;
          ctx.globalAlpha = 0.45;
          ctx.beginPath();
          ctx.arc(i.x, i.y, 5, 0, Math.PI * 2);
          ctx.fill();
          ctx.globalAlpha = 1;
        }
      }
    };

    let raf = 0;
    const loop = () => {
      raf = requestAnimationFrame(loop);
      controls.update();
      const dt = Math.min(clock.getDelta(), 0.1);
      mixer?.update(dt);
      if (running) simTime += dt;
      animated?.update(simTime);
      composer.render();
      if (live.gizmo) drawGizmo();
    };

    const configure = (c: ViewerCfg) => {
      live = c;
      const [env, k, r, f] = LIGHTS[c.light];
      scene.environmentIntensity = env * c.exposure;
      key.intensity = k * c.exposure;
      rim.intensity = r * c.exposure;
      hemi.intensity = f * c.exposure;
      bloom.enabled = c.glow;
      smaa.enabled = c.smaa;
      outlines?.setVisible(c.outlines);
      (scene.background as THREE.Color).set(BG_COLOR[c.bg]);
      if (camera.fov !== c.fov) {
        camera.fov = c.fov;
        camera.updateProjectionMatrix();
      }
      controls.autoRotate = c.spin;
      controls.autoRotateSpeed = 2.2;
      if (grid) grid.visible = c.floor;
      model?.traverse((o) => {
        const m = o as THREE.Mesh;
        if (!m.isMesh) return;
        for (const mat of Array.isArray(m.material) ? m.material : [m.material]) (mat as THREE.MeshStandardMaterial).wireframe = c.wire;
      });
    };

    const reset = () => {
      camera.position.copy(home.pos);
      controls.target.copy(home.target);
      controls.update();
    };
    api.current = {
      reset,
      configure,
      snap: (x, y) => {
        let best: (typeof hits)[number] | null = null;
        let d = 14;
        for (const h of hits) {
          const dd = Math.hypot(h.x - x, h.y - y);
          if (dd <= d) {
            d = dd;
            best = h;
          }
        }
        if (!best) return;
        const dist = camera.position.distanceTo(controls.target);
        // straight up or down needs a nudge, or the orbit has no side to lean on
        const dir = best.dir.clone().add(new THREE.Vector3(0.0001, 0, 0.0001)).normalize();
        camera.position.copy(controls.target).addScaledVector(dir, dist);
        controls.update();
      },
      shot: () => {
        composer.render();
        return new Promise((res) => renderer.domElement.toBlob(res, "image/png"));
      },
      stream: () => renderer.domElement.captureStream(30),
      playTracks: (animation) => {
        if (!model || !mixer) return;
        action?.stop();
        action = null;
        mixer.stopAllAction();
        // the bones go back to the still pose the model was loaded in
        for (const r of restPose) {
          r.bone.position.copy(r.position);
          r.bone.quaternion.copy(r.quaternion);
          r.bone.scale.copy(r.scale);
        }
        if (!animation) return;
        action = mixer.clipAction(createPreviewClip(model, animation));
        action.setLoop(THREE.LoopRepeat, Infinity);
        action.reset().play();
        mixer.timeScale = running ? 1 : 0;
        clock.getDelta();
      },
      setPlaying: (on) => {
        running = on;
        if (mixer) mixer.timeScale = on ? 1 : 0;
      },
      seek: (t) => {
        if (!mixer || !action) return;
        action.time = Math.max(0, Math.min(t, action.getClip().duration));
        const ts = mixer.timeScale;
        mixer.timeScale = 1;
        mixer.update(0);
        mixer.timeScale = ts;
      },
      info: () => ({ time: action?.time ?? 0, dur: action?.getClip().duration ?? 0 }),
      show: async (buf, keepView) => {
        // Preparing a model takes several awaits; when the hero is switched meanwhile, only the newest request
        // may touch the scene, or an older hero drawn late replaces the one that was asked for.
        const ticket = ++showTicket;
        const gltf = await new GLTFLoader().parseAsync(buf.slice(0), "");
        if (ticket !== showTicket) {
          disposeTree(gltf.scene);
          return null;
        }
        animated?.dispose();
        animated = null;
        outlines?.dispose();
        outlines = null;
        if (model) {
          scene.remove(model);
          disposeTree(model);
        }
        mixer?.stopAllAction();
        mixer = null;
        action = null;
        if (grid) {
          scene.remove(grid);
          grid.geometry.dispose();
          (grid.material as THREE.Material).dispose();
          grid = null;
        }
        model = gltf.scene;
        // The model is drawn as the exporter describes it (sidedness, blending, emissive strength); the game's
        // own material animation and the toon outline are layered on top, as Deadlock Mod Manager does.
        simTime = 0;
        const installed = await installAnimatedMaterials(model, (index) => gltf.parser.getDependency("texture", index));
        if (ticket !== showTicket) {
          installed.dispose();
          disposeTree(gltf.scene);
          if (model === gltf.scene) model = null;
          return null;
        }
        animated = installed;
        if (animated.diagnostics.length > 0) console.warn("hero materials", animated.diagnostics);
        outlines = attachNprOutlines(model);
        outlines.setViewportHeight(Math.max(el.clientHeight, 1));
        outlines.setVisible(live.outlines);
        scene.add(model);
        const read = readGroups(model);
        table.current = read.groups;
        applyGroups(model, read.groups, read.chosen);
        // The model stands in a still pose; the animation chosen in the viewer is decoded as bone tracks and
        // played on top of it, so the pose the bones start from is kept to return to.
        mixer = new THREE.AnimationMixer(model);
        restPose = [];
        model.traverse((o) => {
          if ((o as THREE.Bone).isBone) restPose.push({ bone: o as THREE.Bone, position: o.position.clone(), quaternion: o.quaternion.clone(), scale: o.scale.clone() });
        });
        setChosen(read.chosen);
        const box = new THREE.Box3().setFromObject(model);
        const size = box.getSize(new THREE.Vector3());
        const mid = box.getCenter(new THREE.Vector3());
        const r = Math.max(size.x, size.y, size.z) || 1;
        const dist = (r / 2 / Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) * 1.25;
        // the game's models face +X: look at the hero from the front, slightly to one side and above
        const dir = new THREE.Vector3(0.88, 0.12, 0.46).normalize();
        home = { pos: mid.clone().addScaledVector(dir, dist), target: mid };
        camera.near = r / 100;
        camera.far = r * 100;
        camera.updateProjectionMatrix();
        controls.minDistance = r * 0.3;
        controls.maxDistance = r * 6;
        if (!keepView) reset();
        grid = new THREE.GridHelper(r * 3, 24, 0x8a90a0, 0x8a90a0);
        grid.position.set(mid.x, box.min.y, mid.z);
        const gm = grid.material as THREE.LineBasicMaterial;
        gm.transparent = true;
        gm.opacity = 0.22;
        scene.add(grid);
        configure(live);
        return { groups: read.groups };
      },
      choose: (c) => model && applyGroups(model, table.current, c),
    };
    configure(live);
    loop();
    setReady(true);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      controls.dispose();
      animated?.dispose();
      outlines?.dispose();
      composer.dispose();
      if (model) disposeTree(model);
      scene.environment?.dispose();
      pmrem.dispose();
      room.dispose();
      renderer.dispose();
      renderer.domElement.remove();
      api.current = null;
    };
  }, []);

  useEffect(() => {
    if (!ready) return;
    let stale = false;
    // switching to another skin of the hero that is already shown keeps the camera and the veil away
    const same = loaded.current?.hero === hero && loaded.current.addon === addon;
    setSoft(same);
    setState("loading");
    fetchModel(hero, codes, addon)
      // an answer that arrives after the hero was switched again is dropped before it reaches the scene
      .then((buf) => (stale ? null : api.current?.show(buf, same)))
      .then((res) => {
        if (stale) return;
        loaded.current = { hero, addon };
        setGroups(res?.groups ?? []);
        setPlaying(true);
        api.current?.setPlaying(true);
        setShownTick((n) => n + 1);
        setSoft(false);
        setState("ready");
      })
      .catch((e) => {
        if (stale) return;
        { const raw = e instanceof Error ? e.message : String(e); console.error("hero model", raw); setError(t(`skins.err.${raw.split(":")[0]}`, { defaultValue: t("skins.err.generic") })); }
        setState("error");
      });
    return () => {
      stale = true;
    };
  }, [hero, addon, ready]);

  // what the hero can do: its abilities in English first (clip names are matched against them), then the
  // movements and abilities the game's clips make up; the idle movement plays by default
  useEffect(() => {
    let stale = false;
    setChoices([]);
    setActionId(null);
    setAbilitiesEn([]);
    (async () => {
      const en = await call<Ability[]>("hero_abilities", { hero, lang: "english" }).catch(() => [] as Ability[]);
      if (stale) return;
      setAbilitiesEn(en ?? []);
      const list = await call<PreviewAnimationChoice[]>("hero_animations", { hero, abilityNames: (en ?? []).map((a) => a.name) }).catch(() => [] as PreviewAnimationChoice[]);
      if (stale) return;
      setChoices(list ?? []);
      setActionId((list ?? []).some((c) => c.id === "idle") ? "idle" : null);
    })();
    return () => {
      stale = true;
    };
  }, [hero]);

  // the chosen action is decoded as bone tracks and played on the model that is in the scene; a choice that is
  // switched away from before its tracks arrive is dropped
  useEffect(() => {
    if (!ready || shownTick === 0) return;
    const choice = actionId ? choices.find((c) => c.id === actionId) : undefined;
    if (!choice) {
      api.current?.playTracks(null);
      setAnimBusy(false);
      return;
    }
    let stale = false;
    setAnimBusy(true);
    call<PreviewAnimation>("hero_animation", { hero, codes, addon, clipPaths: choice.clips }).then(
      (a) => {
        if (stale) return;
        api.current?.playTracks(a);
        setAnimBusy(false);
      },
      (e) => {
        if (stale) return;
        console.error("hero animation", e);
        api.current?.playTracks(null);
        setAnimBusy(false);
      },
    );
    return () => {
      stale = true;
    };
  }, [ready, shownTick, actionId, choices, hero, addon]);

  // the four abilities of the hero: name in the app's language, icon, and the animation that shows each one
  useEffect(() => {
    let stale = false;
    setAbilities([]);
    call<Ability[]>("hero_abilities", { hero, lang: ru ? "russian" : "english" }).then(
      (r) => !stale && setAbilities(r ?? []),
      () => {},
    );
    return () => {
      stale = true;
    };
  }, [hero, ru]);

  // Switches the game uses internally (ammo counts, reload poses, UI overlays) mean nothing to a person
  // looking at a skin; keep the few groups that change how the hero looks.
  const technical = /ammo|reload|clip|weapon|ui_|_ui|stake|drum|crossbow|toggle|quiver|sewer|banner|gun|default|hide|debug|fx|vfx|lod/i;
  const shown = groups.filter((g) => g.choices.length <= 6 && !technical.test(g.name) && !g.choices.some((c) => technical.test(c.name)));

  const up = (patch: Partial<ViewerCfg>) => setCfg((c) => ({ ...c, ...patch }));
  useEffect(() => {
    api.current?.configure(cfg);
    storeCfg(cfg);
  }, [cfg, ready]);

  // timeline position, read from the player a few times a second
  useEffect(() => {
    if (!ready || choices.length === 0) return;
    const id = window.setInterval(() => {
      const i = api.current?.info();
      if (i) setPos((p) => (Math.abs(p.time - i.time) < 0.01 && p.dur === i.dur ? p : i));
    }, 90);
    return () => window.clearInterval(id);
  }, [ready, choices.length]);

  useEffect(() => {
    const on = () => setFull(document.fullscreenElement === view.current);
    document.addEventListener("fullscreenchange", on);
    return () => document.removeEventListener("fullscreenchange", on);
  }, []);

  // closes an open menu on a click elsewhere or Escape
  useEffect(() => {
    if (!menu) return;
    const down = (e: PointerEvent) => {
      if (!(e.target as Element).closest(".vw-pop, .vw-menu-btn")) setMenu(null);
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && setMenu(null);
    document.addEventListener("pointerdown", down);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", down);
      document.removeEventListener("keydown", key);
    };
  }, [menu]);

  useEffect(() => {
    if (!rec) return;
    const id = window.setInterval(() => setSecs(Math.floor((Date.now() - rec.at) / 1000)), 250);
    return () => window.clearInterval(id);
  }, [rec]);
  useEffect(() => {
    if (!note) return;
    const id = window.setTimeout(() => setNote(""), 2600);
    return () => window.clearTimeout(id);
  }, [note]);
  useEffect(() => () => recorder.current?.stop(), []);

  const fileBase = hero + (addon ? "-" + addon.replace(/\.[^.]+$/, "") : "");

  const screenshot = async () => {
    const blob = await api.current?.shot();
    if (!blob) return;
    try {
      if (await saveCapture(blob, fileBase, "png", t("skins.vw.saveShot"))) setNote(t("skins.vw.saved"));
    } catch {
      setNote(t("skins.vw.saveFailed"));
    }
  };

  const toggleRecord = () => {
    if (recorder.current) {
      recorder.current.stop();
      return;
    }
    const a = api.current;
    if (!a || typeof MediaRecorder === "undefined") return setNote(t("skins.vw.noRecord"));
    const type = ["video/webm;codecs=vp9", "video/webm;codecs=vp8", "video/webm"].find((m) => MediaRecorder.isTypeSupported(m));
    if (!type) return setNote(t("skins.vw.noRecord"));
    const chunks: Blob[] = [];
    const r = new MediaRecorder(a.stream(), { mimeType: type, videoBitsPerSecond: 8_000_000 });
    r.ondataavailable = (e) => e.data.size > 0 && chunks.push(e.data);
    r.onstop = () => {
      recorder.current = null;
      setRec(null);
      saveCapture(new Blob(chunks, { type: "video/webm" }), fileBase, "webm", t("skins.vw.saveVideo")).then(
        (ok) => ok && setNote(t("skins.vw.saved")),
        () => setNote(t("skins.vw.saveFailed")),
      );
    };
    r.start(500);
    recorder.current = r;
    setSecs(0);
    setRec({ at: Date.now() });
  };

  const fullscreen = () => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void view.current?.requestFullscreen().catch(() => {});
  };

  const pickAction = (v: string) => setActionId(v === "" ? null : v);
  // each ability of the hero with the action that animates it; actions no slot claimed are still listed
  const slots = assignAbilitySlots(choices, abilitiesEn.map((a) => ({ className: a.class, name: a.name }))).slots;
  const idleId = choices.some((c) => c.id === "idle") ? "idle" : null;
  const abilityLabel = (c: PreviewAnimationChoice) => {
    const i = slots.findIndex((s) => s.action?.id === c.id);
    return (i >= 0 ? abilities[i]?.name : undefined) ?? clipLabel(c.id.replace(/^ability:/, ""), ru);
  };
  const actionOptions = [
    { value: "", label: t("skins.vw.restPose") },
    ...choices.filter((c) => c.kind === "movement").map((c) => ({ value: c.id, label: clipLabel(c.id, ru), group: t("skins.vw.movement") })),
    ...choices.filter((c) => c.kind === "ability").map((c) => ({ value: c.id, label: abilityLabel(c), group: t("skins.vw.abilityGroup") })),
  ];
  const togglePlay = () => {
    const next = !playing;
    setPlaying(next);
    api.current?.setPlaying(next);
  };

  const ready2 = state === "ready";
  return (
    <div ref={view} className={"skin-viewer" + (full ? " is-full" : "")}>
      <div ref={host} className="skin-canvas" title={t("skins.controls")} />
      {state === "loading" && !soft && <div className="skin-veil">{t("skins.loading")}</div>}
      {state === "loading" && soft && <div className="vw-busy">{t("skins.vw.busy")}</div>}
      {state === "error" && (
        <div className="skin-veil skin-veil-error">
          <b>{t("skins.previewFailed")}</b>
          <span>{error}</span>
        </div>
      )}

      <div className="vw-top">
        <div className="vw-menus">
          <button className="vw-pill vw-menu-btn" aria-expanded={menu === "view"} onClick={() => setMenu(menu === "view" ? null : "view")}>
            <Icon name="sliders" size={15} />
            {t("skins.vw.view")}
          </button>
          <button className="vw-pill vw-menu-btn" aria-expanded={menu === "studio"} onClick={() => setMenu(menu === "studio" ? null : "studio")}>
            <Icon name="image" size={15} />
            {t("skins.vw.studio")}
          </button>
          <button className="vw-tag" aria-pressed={cfg.smaa} title={t("skins.vw.smaaHint")} onClick={() => up({ smaa: !cfg.smaa })}>
            SMAA
          </button>
        </div>
        {cfg.gizmo && (
          <canvas
            ref={gizmo}
            className="vw-gizmo"
            style={{ width: GIZMO, height: GIZMO }}
            title={t("skins.vw.gizmoHint")}
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              api.current?.snap(e.clientX - r.left, e.clientY - r.top);
            }}
          />
        )}
      </div>

      {menu === "view" && (
        <div className="vw-pop" role="dialog" aria-label={t("skins.vw.view")}>
          <Row label={t("skins.vw.wire")} on={cfg.wire} onChange={(v) => up({ wire: v })} />
          <Row label={t("skins.vw.floor")} on={cfg.floor} onChange={(v) => up({ floor: v })} />
          <Row label={t("skins.vw.gizmo")} on={cfg.gizmo} onChange={(v) => up({ gizmo: v })} />
          <Row label={t("skins.vw.spin")} on={cfg.spin} onChange={(v) => up({ spin: v })} />
          {shown.length > 0 && ready2 && <Row label={t("skins.variants")} on={showVariants} onChange={setShowVariants} />}
          <Slide label={t("skins.vw.fov")} min={20} max={70} step={1} value={cfg.fov} fmt={(v) => v + "°"} onChange={(v) => up({ fov: v })} />
          <button className="btn btn-sm vw-reset" onClick={() => api.current?.reset()}>
            {t("skins.resetView")}
          </button>
        </div>
      )}
      {menu === "studio" && (
        <div className="vw-pop" role="dialog" aria-label={t("skins.vw.studio")}>
          <Pick label={t("skins.vw.bg")} value={cfg.bg} options={BGS.map((b) => ({ value: b, label: t("skins.vw.bg_" + b) }))} onChange={(v) => up({ bg: v as Bg })} />
          <Pick label={t("skins.vw.light")} value={cfg.light} options={LIGHT_KEYS.map((b) => ({ value: b, label: t("skins.vw.light_" + b) }))} onChange={(v) => up({ light: v as LightPreset })} />
          <Slide label={t("skins.vw.exposure")} min={0.5} max={1.8} step={0.05} value={cfg.exposure} fmt={(v) => v.toFixed(2)} onChange={(v) => up({ exposure: v })} />
          <Row label={t("skins.vw.glow")} on={cfg.glow} onChange={(v) => up({ glow: v })} />
          <Row label={t("skins.vw.outlines")} on={cfg.outlines} onChange={(v) => up({ outlines: v })} />
          <button className="btn btn-sm vw-reset" onClick={() => setCfg(DEFAULT_CFG)}>
            {t("skins.vw.defaults")}
          </button>
        </div>
      )}

      <div className="vw-side">
        <button className="vw-ico" aria-label={t("skins.resetView")} title={t("skins.resetView")} onClick={() => api.current?.reset()}>
          <Icon name="frame" size={17} />
        </button>
        <button className="vw-ico" aria-pressed={cfg.spin} aria-label={t("skins.vw.spin")} title={t("skins.vw.spin")} onClick={() => up({ spin: !cfg.spin })}>
          <Icon name="spin" size={17} />
        </button>
        <button className="vw-ico" aria-label={t("skins.vw.shot")} title={t("skins.vw.shot")} disabled={!ready2} onClick={() => void screenshot()}>
          <Icon name="camera" size={17} />
        </button>
        <button className={"vw-ico" + (rec ? " is-rec" : "")} aria-pressed={!!rec} aria-label={rec ? t("skins.vw.stop") : t("skins.vw.record")} title={rec ? t("skins.vw.stop") : t("skins.vw.record")} disabled={!ready2} onClick={toggleRecord}>
          <Icon name="video" size={17} />
        </button>
        <button className="vw-ico" aria-pressed={full} aria-label={t("skins.vw.full")} title={t("skins.vw.full")} onClick={fullscreen}>
          <Icon name="expand" size={17} />
        </button>
      </div>

      {rec && (
        <div className="vw-rec" role="status">
          <i /> {clock2(secs)}
        </div>
      )}
      {note && (
        <div className="vw-note" role="status">
          {note}
        </div>
      )}

      {shown.length > 0 && ready2 && showVariants && (
        <div className="skin-variants">
          {shown.map((g) => (
            <div key={g.name} className="skin-variant-row">
              <span>{g.name}</span>
              {g.choices.map((c) => (
                <button
                  key={c.bit}
                  className="chip"
                  aria-pressed={chosen[g.name] === c.bit}
                  onClick={() => {
                    const next = { ...chosen, [g.name]: c.bit };
                    setChosen(next);
                    api.current?.choose(next);
                  }}
                >
                  {c.name}
                </button>
              ))}
            </div>
          ))}
        </div>
      )}

      {abilities.length > 0 && (ready2 || soft) && (
        <div className="vw-slots" role="group" aria-label={t("skins.vw.abilities")}>
          {abilities.map((a, i) => {
            const act = slots[i]?.action ?? null;
            return (
            <button
              key={a.class}
              className="vw-slot"
              aria-pressed={act !== null && actionId === act.id}
              disabled={!act}
              title={act ? a.name : t("skins.vw.noClip", { name: a.name })}
              onClick={() => act && pickAction(actionId === act.id ? (idleId ?? "") : act.id)}
            >
              <span className="vw-slot-box">
                {a.icon ? <img src={a.icon} alt="" loading="lazy" draggable={false} /> : <b>{i + 1}</b>}
                <i className="vw-slot-n">{i + 1}</i>
              </span>
              <span className="vw-slot-name">{a.name}</span>
            </button>
            );
          })}
        </div>
      )}

      {choices.length > 0 && (ready2 || soft) && (
        <div className="vw-anim">
          <button className="vw-play" aria-label={playing ? t("skins.vw.pause") : t("skins.vw.play")} onClick={togglePlay}>
            <Icon name={playing ? "pause" : "play"} size={16} />
          </button>
          <Select className="vw-clip" label={t("skins.vw.clip")} value={actionId ?? ""} onChange={pickAction} options={actionOptions} />
          {animBusy && <span className="vw-busy-dot" aria-hidden="true" />}
          <span className="vw-time">{clock2(pos.time)}</span>
          <input
            className="vw-scrub"
            type="range"
            min={0}
            max={pos.dur || 1}
            step={0.01}
            value={Math.min(pos.time, pos.dur || 1)}
            aria-label={t("skins.vw.timeline")}
            onChange={(e) => {
              const v = Number(e.target.value);
              setPos((p) => ({ ...p, time: v }));
              api.current?.seek(v);
            }}
          />
          <span className="vw-time vw-dim">{clock2(pos.dur)}</span>
        </div>
      )}
    </div>
  );
}

const BGS: Bg[] = ["dark", "slate", "light"];
const LIGHT_KEYS: LightPreset[] = ["studio", "soft", "dramatic"];

function Row({ label, on, onChange }: { label: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="vw-row">
      <span>{label}</span>
      <input type="checkbox" role="switch" className="vw-switch" checked={on} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

function Slide({ label, min, max, step, value, fmt, onChange }: { label: string; min: number; max: number; step: number; value: number; fmt: (v: number) => string; onChange: (v: number) => void }) {
  return (
    <label className="vw-row vw-slide">
      <span>{label}</span>
      <input type="range" min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} />
      <b>{fmt(value)}</b>
    </label>
  );
}

function Pick({ label, value, options, onChange }: { label: string; value: string; options: { value: string; label: string }[]; onChange: (v: string) => void }) {
  return (
    <div className="vw-pick">
      <span>{label}</span>
      <div className="vw-segs" role="radiogroup" aria-label={label}>
        {options.map((o) => (
          <button key={o.value} role="radio" aria-checked={value === o.value} className="vw-seg" onClick={() => onChange(o.value)}>
            {o.label}
          </button>
        ))}
      </div>
    </div>
  );
}
