import { useCallback, useEffect, useRef, useState, type PointerEvent } from "react";
import {
  setViewOptions,
  setViewportRect,
  viewportDrag,
  viewportGizmo,
  viewportPick,
  viewportResetCamera,
  viewportZoom,
  type FrameState,
  type Gizmo,
  type ValueEdit,
  type ViewMode,
} from "./backend";

/** Movement in pixels below which a press counts as a click, not a drag. */
const CLICK_DISTANCE = 4;

/** Colours of the axes as the original shows them with "Z is up" off. */
const AXIS_COLORS: Record<string, string> = { X: "#ff4d4d", Y: "#4d8dff", Z: "#5fd35f" };

type Tool = "move" | "rotate" | "scale";

interface Props {
  selected: string | null;
  /** The evaluated frame; the controls follow it. */
  frame: FrameState | null;
  /** A click selected a timeline, or hit nothing (null). */
  onPick: (id: string | null, keepSelection: boolean) => void;
  /** Changes values of the selected timeline; a drag sends its total with mode "add". */
  onEditValues: (values: ValueEdit[], mode: "set" | "add", merge: string | null) => void;
  onEditDone: () => void;
}

/** What a control being dragged has changed so far. */
interface ControlDrag {
  pointer: number;
  value: string;
  total: number;
}

/** GameMaker's `point_direction`: degrees, counter-clockwise, with y down. */
const pointDirection = (x1: number, y1: number, x2: number, y2: number) =>
  (Math.atan2(-(y2 - y1), x2 - x1) * 180) / Math.PI;

/** `angle_difference_fix`: the signed difference of two angles in -180..180. */
const angleDifference = (a: number, b: number) => ((((a - b) % 360) + 540) % 360) - 180;

/**
 * The 3D view. The scene itself is drawn by the backend straight onto the
 * window, underneath the webview; this element is a transparent hole that
 * tells the backend where to draw and forwards mouse input. The controls of
 * the selected timeline are drawn here, over the scene.
 */
export function Viewport({ selected, frame, onPick, onEditValues, onEditDone }: Props) {
  const element = useRef<HTMLDivElement>(null);
  const [mode, setMode] = useState<ViewMode>("shaded");
  const [timelineCamera, setTimelineCamera] = useState(false);
  const [tool, setTool] = useState<Tool>("move");
  const [gizmo, setGizmo] = useState<Gizmo | null>(null);
  // Bumped whenever the camera or the viewport changed, to fetch the controls again.
  const [cameraTick, setCameraTick] = useState(0);
  const cameraMoved = useCallback(() => setCameraTick((t) => t + 1), []);

  // Keep the backend informed of where the hole is.
  useEffect(() => {
    const target = element.current;
    if (!target) return;
    const report = () => {
      const rect = target.getBoundingClientRect();
      const scale = window.devicePixelRatio;
      void setViewportRect(rect.left * scale, rect.top * scale, rect.width * scale, rect.height * scale).then(cameraMoved);
    };
    const observer = new ResizeObserver(report);
    observer.observe(target);
    window.addEventListener("resize", report);
    report();
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", report);
      void setViewportRect(0, 0, 0, 0);
    };
  }, [cameraMoved]);

  useEffect(() => {
    void setViewOptions(mode, timelineCamera).then(cameraMoved);
  }, [mode, timelineCamera, cameraMoved]);

  // The controls of the selected timeline, where the backend says they are.
  const gizmoRef = useRef<Gizmo | null>(null);
  useEffect(() => {
    if (!selected) {
      gizmoRef.current = null;
      setGizmo(null);
      return;
    }
    let current = true;
    viewportGizmo(selected).then(
      (g) => {
        if (!current) return;
        gizmoRef.current = g;
        setGizmo(g);
      },
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, [selected, frame, cameraTick]);

  // Mouse movement is collected and sent once per animation frame.
  const pending = useRef({ dx: 0, dy: 0, pan: false, scheduled: false });
  const press = useRef<{ moved: number; button: number } | null>(null);
  const flush = () => {
    const drag = pending.current;
    drag.scheduled = false;
    if (drag.dx !== 0 || drag.dy !== 0) {
      void viewportDrag(drag.pan ? "pan" : "orbit", drag.dx, drag.dy).then(cameraMoved);
      drag.dx = 0;
      drag.dy = 0;
    }
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 && event.button !== 1) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    press.current = { moved: 0, button: event.button };
    // Middle button or Shift pans; plain left button orbits.
    pending.current.pan = event.button === 1 || event.shiftKey;
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
    if (press.current) press.current.moved += Math.abs(event.movementX) + Math.abs(event.movementY);
    // Small movements of a click do not turn the camera.
    if (press.current && press.current.moved < CLICK_DISTANCE) return;
    const drag = pending.current;
    drag.dx += event.movementX;
    drag.dy += event.movementY;
    if (!drag.scheduled) {
      drag.scheduled = true;
      requestAnimationFrame(flush);
    }
  };

  // A left click that did not move picks what is under the pointer.
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    const current = press.current;
    press.current = null;
    if (!current || current.button !== 0 || current.moved >= CLICK_DISTANCE) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const scale = window.devicePixelRatio;
    const exact = event.ctrlKey || event.metaKey;
    const keep = event.shiftKey;
    viewportPick((event.clientX - rect.left) * scale, (event.clientY - rect.top) * scale, exact, selected).then(
      (id) => onPick(id, keep),
      () => undefined,
    );
  };

  // React registers wheel listeners as passive, which cannot prevent the
  // page from scrolling, so this one is attached by hand.
  useEffect(() => {
    const target = element.current;
    if (!target) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      void viewportZoom(Math.sign(event.deltaY)).then(cameraMoved);
    };
    target.addEventListener("wheel", onWheel, { passive: false });
    return () => target.removeEventListener("wheel", onWheel);
  }, [cameraMoved]);

  // Dragging a control (`view_control_move`, `view_control_rotate`).
  const control = useRef<ControlDrag | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const scale = window.devicePixelRatio;

  const controlDown = (event: PointerEvent<SVGElement>, value: string) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    control.current = { pointer: event.pointerId, value, total: 0 };
    setActive(value);
  };

  const controlMove = (event: PointerEvent<SVGElement>) => {
    const drag = control.current;
    const g = gizmoRef.current;
    if (!drag || drag.pointer !== event.pointerId || !g?.center) return;
    event.stopPropagation();
    if (event.movementX === 0 && event.movementY === 0) return;

    const axis = g.move.find((a) => a.value === drag.value) ?? g.scale.find((a) => a.value === drag.value);
    if (axis) {
      // How far along the arrow's direction on screen the mouse went, as a
      // share of the arrow, is how far along it the object moves.
      const vec = [(axis.end[0] - g.center[0]) / scale, (axis.end[1] - g.center[1]) / scale];
      const lengthSquared = vec[0] * vec[0] + vec[1] * vec[1];
      if (lengthSquared === 0) return;
      const along = (event.movementX * vec[0] + event.movementY * vec[1]) / lengthSquared;
      drag.total += (along * axis.length) / axis.scale;
    }
    const ring = g.rotate.find((r) => r.value === drag.value);
    if (ring) {
      const rect = element.current!.getBoundingClientRect();
      const [cx, cy] = [g.center[0] / scale, g.center[1] / scale];
      const [x, y] = [event.clientX - rect.left, event.clientY - rect.top];
      const now = pointDirection(x, y, cx, cy);
      const before = pointDirection(x - event.movementX, y - event.movementY, cx, cy);
      drag.total += angleDifference(now, before) * (ring.flip ? -1 : 1);
    }
    const total = Math.round(drag.total * 1000) / 1000;
    onEditValues([{ name: drag.value, value: total }], "add", `control:${drag.value}`);
  };

  const controlUp = (event: PointerEvent<SVGElement>) => {
    const drag = control.current;
    if (!drag || drag.pointer !== event.pointerId) return;
    event.stopPropagation();
    control.current = null;
    setActive(null);
    onEditDone();
  };

  const handlers = (value: string) => ({
    onPointerDown: (e: PointerEvent<SVGElement>) => controlDown(e, value),
    onPointerMove: controlMove,
    onPointerUp: controlUp,
    onPointerCancel: controlUp,
  });
  const css = (p: [number, number]) => [p[0] / scale, p[1] / scale] as const;
  const colorOf = (value: string) => (active === value ? "#ffffff" : AXIS_COLORS[value.slice(-1)]);

  return (
    <div className="viewport-cell">
      <div className="viewport-bar">
        <select
          value={timelineCamera ? "active" : "work"}
          onChange={(e) => setTimelineCamera(e.target.value === "active")}
          title="Camera this view looks through"
        >
          <option value="work">Work camera</option>
          <option value="active">Active camera</option>
        </select>
        <button className="secondary" onClick={() => void viewportResetCamera().then(cameraMoved)} disabled={timelineCamera}>
          Reset view
        </button>
        <div className="tool-switch" role="group" aria-label="Tool">
          <button className={tool === "move" ? "active" : ""} onClick={() => setTool("move")} title="Move the selection">
            Move
          </button>
          <button className={tool === "rotate" ? "active" : ""} onClick={() => setTool("rotate")} title="Rotate the selection">
            Rotate
          </button>
          <button className={tool === "scale" ? "active" : ""} onClick={() => setTool("scale")} title="Scale the selection">
            Scale
          </button>
        </div>
        <span className="spacer" />
        <label>
          Shading
          <select value={mode} onChange={(e) => setMode(e.target.value as ViewMode)}>
            <option value="flat">Flat</option>
            <option value="shaded">Shaded</option>
            <option value="render">Render</option>
          </select>
        </label>
      </div>
      <div
        className="viewport"
        ref={element}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onContextMenu={(event) => event.preventDefault()}
      >
        {gizmo?.center && (
          <svg className="gizmo">
            {tool === "move" &&
              gizmo.move.map((axis) => {
                const [x1, y1] = css(axis.start);
                const [x2, y2] = css(axis.end);
                const angle = (Math.atan2(y2 - y1, x2 - x1) * 180) / Math.PI;
                return (
                  <g key={axis.value} className="gizmo-control" {...handlers(axis.value)}>
                    {/* A wide invisible line makes the arrow easy to grab. */}
                    <line x1={x1} y1={y1} x2={x2} y2={y2} className="gizmo-hit" />
                    <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={colorOf(axis.value)} className="gizmo-line" />
                    <polygon
                      points="0,-5 12,0 0,5"
                      fill={colorOf(axis.value)}
                      transform={`translate(${x2} ${y2}) rotate(${angle})`}
                    />
                  </g>
                );
              })}
            {tool === "scale" &&
              gizmo.scale.map((axis) => {
                const [x1, y1] = css(axis.start);
                const [x2, y2] = css(axis.end);
                return (
                  <g key={axis.value} className="gizmo-control" {...handlers(axis.value)}>
                    <line x1={x1} y1={y1} x2={x2} y2={y2} className="gizmo-hit" />
                    <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={colorOf(axis.value)} className="gizmo-line" />
                    <rect x={x2 - 5} y={y2 - 5} width={10} height={10} fill={colorOf(axis.value)} />
                  </g>
                );
              })}
            {tool === "rotate" &&
              gizmo.rotate.map((ring) => {
                const points = ring.points.map((p) => css(p).join(",")).join(" ");
                return (
                  <g key={ring.value} className="gizmo-control" {...handlers(ring.value)}>
                    <polyline points={points} className="gizmo-hit" />
                    <polyline points={points} stroke={colorOf(ring.value)} className="gizmo-line" />
                  </g>
                );
              })}
          </svg>
        )}
      </div>
    </div>
  );
}
