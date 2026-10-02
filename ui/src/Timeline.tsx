import { useCallback, useRef, useState, type PointerEvent } from "react";
import type { FrameState, KeyframeKey, ProjectSummary } from "./backend";

const ROW_HEIGHT = 24;
const RULER_HEIGHT = 26;
const MIN_FRAMES = 120;
/** Space before frame 0 so that keyframes there are not cut off. */
const PADDING = 10;

/** Identifies a keyframe in sets. */
export const keyframeId = (key: KeyframeKey) => `${key.timeline}:${key.position}`;

interface Props {
  project: ProjectSummary;
  frame: FrameState | null;
  marker: number;
  selected: string | null;
  /** Selected keyframes, by `keyframeId`. */
  selectedKeyframes: Set<string>;
  playing: boolean;
  onSeek: (marker: number) => void;
  onSelect: (id: string) => void;
  onSelectKeyframes: (keys: KeyframeKey[]) => void;
  /** Called while dragging with the total offset from where the drag began. */
  onMoveKeyframes: (keys: KeyframeKey[], offset: number) => void;
  onMoveDone: () => void;
  onRename: (id: string, name: string) => void;
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
  const { project, frame, marker, selected, selectedKeyframes, playing, onSeek, onSelect, onPlay } = props;
  const tracks = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const [zoom, setZoom] = useState(12);
  const [search, setSearch] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
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
    onSelect(key.timeline);
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
        <div className="timeline-list">
          <div className="timeline-search" style={{ height: RULER_HEIGHT }}>
            <input type="search" placeholder="Search…" value={search} onChange={(e) => setSearch(e.target.value)} />
          </div>
          {rows.map(({ timeline, index }) => {
            const state = frame?.timelines[index];
            const classes = ["timeline-row"];
            if (timeline.id === selected) classes.push("selected");
            if (timeline.hidden || state?.visible === false) classes.push("dimmed");
            return (
              <div
                key={timeline.id}
                className={classes.join(" ")}
                style={{ height: ROW_HEIGHT, paddingLeft: 8 + (query ? 0 : timeline.depth * 14) }}
                onClick={() => onSelect(timeline.id)}
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

        <div className="timeline-tracks" ref={tracks}>
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
                className={timeline.id === selected ? "track selected" : "track"}
                style={{ height: ROW_HEIGHT }}
                onClick={() => {
                  onSelect(timeline.id);
                  props.onSelectKeyframes([]);
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
