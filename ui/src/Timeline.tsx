import { useCallback, useEffect, useRef, useState, type PointerEvent } from "react";
import type { CreatableKind, FrameState, KeyframeKey, ProjectSummary, TimelineSummary } from "./backend";
import { Workbench } from "./Workbench";

const ROW_HEIGHT = 24;
const RULER_HEIGHT = 26;
const MIN_FRAMES = 120;
/** Space before frame 0 so that keyframes there are not cut off. */
const PADDING = 10;

/** Where a dragged timeline would land relative to the row under the pointer. */
type DropZone = "before" | "into" | "after";

interface RowDrag {
  pointer: number;
  id: string;
  startY: number;
  dragging: boolean;
  target: { id: string | null; zone: DropZone } | null;
}

/** Whether `id` is `ancestor` or below it. */
function isWithin(timelines: TimelineSummary[], id: string, ancestor: string): boolean {
  let current: string | null = id;
  while (current) {
    if (current === ancestor) return true;
    current = timelines.find((t) => t.id === current)?.parent ?? null;
  }
  return false;
}

/** How a click changes the selected timelines. */
export type SelectMode = "only" | "toggle" | "add" | "range";

/** Identifies a keyframe in sets. */
export const keyframeId = (key: KeyframeKey) => `${key.timeline}:${key.position}`;

interface Props {
  project: ProjectSummary;
  frame: FrameState | null;
  marker: number;
  /** Selected timelines; the last one is the one being edited. */
  selection: string[];
  /** Selected keyframes, by `keyframeId`. */
  selectedKeyframes: Set<string>;
  playing: boolean;
  onSeek: (marker: number) => void;
  /** The frame under the mouse in the tracks, or null when it is elsewhere. */
  onHoverFrame: (frame: number | null) => void;
  onSelect: (id: string, mode: SelectMode) => void;
  onSelectKeyframes: (keys: KeyframeKey[]) => void;
  /** Called while dragging with the total offset from where the drag began. */
  onMoveKeyframes: (keys: KeyframeKey[], offset: number) => void;
  onMoveDone: () => void;
  onRename: (id: string, name: string) => void;
  onCreate: (kind: CreatableKind) => void;
  onCreateModel: (name: string) => void;
  onCreateBlock: (name: string) => void;
  onCreateItem: (name: string) => void;
  onCreateText: () => void;
  /** Asks for a file and adds it as scenery. */
  onCreateScenery: () => void;
  /** Moves timelines under `parent` (the root for null) at `index` among its other children, or at the end. */
  onReparent: (ids: string[], parent: string | null, index: number | null) => void;
  onToggleHidden: (id: string, hidden: boolean) => void;
  onPlay: (playing: boolean) => void;
}

/** `00:00:00.000`: hours, minutes, seconds and milliseconds. */
function timecode(frame: number, tempo: number): string {
  const total = tempo > 0 ? frame / tempo : 0;
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor(total / 60) % 60;
  const seconds = Math.floor(total) % 60;
  const millis = Math.floor((total - Math.floor(total)) * 1000);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(hours)}:${two(minutes)}:${two(seconds)}.${String(millis).padStart(3, "0")}`;
}

interface Drag {
  pointer: number;
  startX: number;
  keys: KeyframeKey[];
  offset: number;
  /** The keyframe the drag began on, to jump to when it was only a click. */
  clicked: KeyframeKey;
}

/** The timeline panel: time and transport on top, timeline list on the left, keyframes on the right. */
export function Timeline(props: Props) {
  const { project, frame, marker, selection, selectedKeyframes, playing, onSeek, onSelect, onPlay } = props;
  const selected = selection.length > 0 ? selection[selection.length - 1] : null;
  const tracks = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const [zoom, setZoom] = useState(12);
  const [search, setSearch] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [creating, setCreating] = useState<DOMRect | null>(null);
  const list = useRef<HTMLDivElement>(null);

  // Keep the selected timeline in view, such as one just created.
  useEffect(() => {
    if (!selected) return;
    list.current?.querySelector(`[data-timeline="${CSS.escape(selected)}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selected, project]);
  const rowDrag = useRef<RowDrag | null>(null);
  const [dropTarget, setDropTarget] = useState<RowDrag["target"]>(null);

  // Rows: a click selects, dragging up or down moves the timeline in the tree.
  const rowDown = (event: PointerEvent<HTMLDivElement>, timeline: TimelineSummary) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest("button, input")) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    // Parts move with their owner only; they can still be selected.
    rowDrag.current = { pointer: event.pointerId, id: timeline.id, startY: event.clientY, dragging: false, target: null };
  };

  const rowMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = rowDrag.current;
    if (!drag || drag.pointer !== event.pointerId) return;
    const dragged = project.timelines.find((t) => t.id === drag.id);
    if (!dragged || dragged.part) return;
    if (!drag.dragging && Math.abs(event.clientY - drag.startY) < 5) return;
    drag.dragging = true;
    const under = document.elementFromPoint(event.clientX, event.clientY);
    const row = under?.closest<HTMLElement>("[data-timeline]");
    let target: RowDrag["target"] = null;
    if (row) {
      const id = row.dataset.timeline!;
      const rect = row.getBoundingClientRect();
      const y = (event.clientY - rect.top) / rect.height;
      const zone: DropZone = y < 0.25 ? "before" : y > 0.75 ? "after" : "into";
      if (!isWithin(project.timelines, id, drag.id)) target = { id, zone };
    } else if (under?.closest(".timeline-list")) {
      // Below the last row: the end of the root.
      target = { id: null, zone: "after" };
    }
    drag.target = target;
    setDropTarget(target);
  };

  const rowUp = (event: PointerEvent<HTMLDivElement>, timeline: TimelineSummary) => {
    const drag = rowDrag.current;
    if (!drag || drag.pointer !== event.pointerId) return;
    rowDrag.current = null;
    setDropTarget(null);
    if (!drag.dragging) {
      onSelect(timeline.id, event.ctrlKey || event.metaKey ? "toggle" : event.shiftKey ? "range" : "only");
      return;
    }
    const target = drag.target;
    if (!target) return;
    // A selected row takes the rest of the selection along.
    const ids = selection.includes(drag.id) ? project.timelines.filter((t) => selection.includes(t.id)).map((t) => t.id) : [drag.id];
    if (target.id === null) {
      props.onReparent(ids, null, null);
      return;
    }
    const over = project.timelines.find((t) => t.id === target.id);
    if (!over || ids.some((id) => isWithin(project.timelines, over.id, id))) return;
    if (target.zone === "into") {
      props.onReparent(ids, over.id, null);
      return;
    }
    // The place among the children that stay.
    const before = project.timelines.filter((t) => t.parent === over.parent && !ids.includes(t.id) && t.index < over.index).length;
    props.onReparent(ids, over.parent, before + (target.zone === "after" ? 1 : 0));
  };
  const frames = Math.max(project.length + Math.ceil(project.tempo), MIN_FRAMES);
  const width = PADDING + frames * zoom;
  const frameX = (f: number) => PADDING + f * zoom;

  const seekFromPointer = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      const element = tracks.current;
      if (!element) return;
      // The tracks element scrolls with its parent, so its bounding box
      // already accounts for the scroll position.
      const x = event.clientX - element.getBoundingClientRect().left;
      onSeek(Math.max(0, Math.round((x - PADDING) / zoom)));
    },
    [onSeek, zoom],
  );

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    seekFromPointer(event);
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) seekFromPointer(event);
  };

  // Keyframes: click selects (Shift or Ctrl adds), dragging moves the selection.
  const keyframeDown = (event: PointerEvent<HTMLSpanElement>, key: KeyframeKey) => {
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    const id = keyframeId(key);
    let keys: KeyframeKey[];
    const current = [...selectedKeyframes].map((s) => {
      const at = s.lastIndexOf(":");
      return { timeline: s.slice(0, at), position: Number(s.slice(at + 1)) };
    });
    if (event.shiftKey || event.ctrlKey) {
      keys = selectedKeyframes.has(id) ? current.filter((k) => keyframeId(k) !== id) : [...current, key];
    } else {
      keys = selectedKeyframes.has(id) ? current : [key];
    }
    props.onSelectKeyframes(keys);
    onSelect(key.timeline, event.shiftKey || event.ctrlKey ? "add" : "only");
    drag.current = { pointer: event.pointerId, startX: event.clientX, keys, offset: 0, clicked: key };
  };

  const keyframeMove = (event: PointerEvent<HTMLSpanElement>) => {
    const current = drag.current;
    if (!current || current.pointer !== event.pointerId) return;
    const offset = Math.round((event.clientX - current.startX) / zoom);
    if (offset !== current.offset) {
      current.offset = offset;
      props.onMoveKeyframes(current.keys, offset);
    }
  };

  const keyframeUp = (event: PointerEvent<HTMLSpanElement>) => {
    const current = drag.current;
    if (!current || current.pointer !== event.pointerId) return;
    drag.current = null;
    if (current.offset === 0) onSeek(current.clicked.position);
    else props.onMoveDone();
  };

  // Keyframe positions of all timelines, for stepping between them.
  const positions = [...new Set(project.timelines.flatMap((t) => t.keyframes))].sort((a, b) => a - b);
  const previousKeyframe = [...positions].reverse().find((p) => p < Math.floor(marker));
  const nextKeyframe = positions.find((p) => p > marker);

  // A label every five frames, fewer when zoomed out.
  const labelStep = zoom >= 10 ? 5 : zoom >= 5 ? 10 : 25;
  const labels: number[] = [];
  for (let f = 0; f <= frames; f += labelStep) labels.push(f);

  const query = search.trim().toLowerCase();
  const rows = project.timelines
    .map((timeline, index) => ({ timeline, index }))
    .filter(({ timeline }) => !query || timeline.name.toLowerCase().includes(query) || timeline.kind.includes(query));

  return (
    <section className="timeline-panel">
      <div className="timeline-bar">
        <span className="timecode">
          <strong>{timecode(marker, project.tempo)}</strong> / {timecode(project.length, project.tempo)}
        </span>
        <span className="spacer" />
        <div className="transport">
          <button onClick={() => onSeek(0)} title="First frame">
            ⏮
          </button>
          <button onClick={() => onSeek(previousKeyframe ?? 0)} title="Previous keyframe">
            ◂
          </button>
          <button onClick={() => onSeek(Math.floor(marker))} title="Stop" disabled={!playing}>
            ■
          </button>
          <button onClick={() => onPlay(!playing)} title={playing ? "Pause" : "Play"} disabled={project.length === 0}>
            {playing ? "❚❚" : "▶"}
          </button>
          <button onClick={() => nextKeyframe !== undefined && onSeek(nextKeyframe)} title="Next keyframe" disabled={nextKeyframe === undefined}>
            ▸
          </button>
          <button onClick={() => onSeek(project.length)} title="Last frame">
            ⏭
          </button>
        </div>
        <span className="spacer" />
        <label className="zoom" title="Timeline zoom">
          <input type="range" min={2} max={32} value={zoom} onChange={(e) => setZoom(Number(e.target.value))} />
        </label>
      </div>

      <div className="timeline">
        <div className="timeline-list" ref={list}>
          <div className="timeline-search" style={{ height: RULER_HEIGHT }}>
            <input type="search" placeholder="Search…" value={search} onChange={(e) => setSearch(e.target.value)} />
            <button className="create-button" title="Workbench: add something to the scene" onClick={(e) => setCreating(creating ? null : e.currentTarget.getBoundingClientRect())}>
              +
            </button>
            {creating && (
              <Workbench
                anchor={creating}
                onCreate={props.onCreate}
                onCreateModel={props.onCreateModel}
                onCreateBlock={props.onCreateBlock}
                onCreateItem={props.onCreateItem}
                onCreateText={props.onCreateText}
                onCreateScenery={props.onCreateScenery}
                onClose={() => setCreating(null)}
              />
            )}
          </div>
          {rows.map(({ timeline, index }) => {
            const state = frame?.timelines[index];
            const classes = ["timeline-row"];
            if (selection.includes(timeline.id)) classes.push("selected");
            if (timeline.hidden || state?.visible === false) classes.push("dimmed");
            if (dropTarget?.id === timeline.id) classes.push(`drop-${dropTarget.zone}`);
            return (
              <div
                key={timeline.id}
                className={classes.join(" ")}
                data-timeline={timeline.id}
                style={{ height: ROW_HEIGHT, paddingLeft: 8 + (query ? 0 : timeline.depth * 14) }}
                onPointerDown={(e) => rowDown(e, timeline)}
                onPointerMove={rowMove}
                onPointerUp={(e) => rowUp(e, timeline)}
                onPointerCancel={() => {
                  rowDrag.current = null;
                  setDropTarget(null);
                }}
                onDoubleClick={() => setRenaming(timeline.id)}
                title={`${timeline.kind}${timeline.hidden ? " (hidden)" : ""}`}
              >
                <span className="kind">{timeline.kind}</span>
                {renaming === timeline.id ? (
                  <input
                    className="rename"
                    autoFocus
                    defaultValue={timeline.name}
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") e.currentTarget.blur();
                      if (e.key === "Escape") setRenaming(null);
                    }}
                    onBlur={(e) => {
                      if (renaming === timeline.id && e.currentTarget.value !== timeline.name) {
                        props.onRename(timeline.id, e.currentTarget.value);
                      }
                      setRenaming(null);
                    }}
                  />
                ) : (
                  <span className="name">{timeline.name || "(unnamed)"}</span>
                )}
                {frame?.activeCamera === timeline.id && <span className="badge">active</span>}
                <button
                  className={timeline.hidden ? "row-eye off" : "row-eye"}
                  title={timeline.hidden ? "Show in the viewport" : "Hide in the viewport"}
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onToggleHidden(timeline.id, !timeline.hidden);
                  }}
                >
                  {timeline.hidden ? "◌" : "●"}
                </button>
              </div>
            );
          })}
        </div>

        <div
          className="timeline-tracks"
          ref={tracks}
          onPointerMove={(e) => {
            const x = e.clientX - e.currentTarget.getBoundingClientRect().left;
            props.onHoverFrame(Math.max(0, Math.round((x - PADDING) / zoom)));
          }}
          onPointerLeave={() => props.onHoverFrame(null)}
        >
          <div style={{ width, minHeight: "100%", position: "relative" }}>
            <div
              className="timeline-ruler"
              style={{ height: RULER_HEIGHT }}
              onPointerDown={onPointerDown}
              onPointerMove={onPointerMove}
            >
              {labels.map((f) => (
                <span key={f} className="ruler-label" style={{ left: frameX(f) }}>
                  {f}
                </span>
              ))}
            </div>

            {rows.map(({ timeline }) => (
              <div
                key={timeline.id}
                className={selection.includes(timeline.id) ? "track selected" : "track"}
                style={{ height: ROW_HEIGHT }}
                onClick={(e) => {
                  onSelect(timeline.id, e.ctrlKey || e.metaKey ? "toggle" : e.shiftKey ? "range" : "only");
                  if (!e.ctrlKey && !e.metaKey && !e.shiftKey) props.onSelectKeyframes([]);
                }}
              >
                {timeline.keyframes.map((position) => {
                  const key = { timeline: timeline.id, position };
                  return (
                    <span
                      key={position}
                      className={selectedKeyframes.has(keyframeId(key)) ? "keyframe selected" : "keyframe"}
                      style={{ left: frameX(position) }}
                      title={`Frame ${position}`}
                      onClick={(e) => e.stopPropagation()}
                      onPointerDown={(e) => keyframeDown(e, key)}
                      onPointerMove={keyframeMove}
                      onPointerUp={keyframeUp}
                      onPointerCancel={keyframeUp}
                    />
                  );
                })}
              </div>
            ))}

            <div className="playhead" style={{ left: frameX(marker) }} />
          </div>
          {project.timelines.length === 0 && (
            <p className="timeline-empty">Nothing here yet! This project has no timelines.</p>
          )}
        </div>
      </div>
    </section>
  );
}
