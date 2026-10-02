import { useCallback, useRef, type PointerEvent } from "react";
import type { FrameState, ProjectSummary } from "./backend";

const ROW_HEIGHT = 24;
const HEADER_HEIGHT = 24;
const MIN_FRAMES = 48;
/** Space before frame 0 so that keyframes there are not cut off. */
const PADDING = 10;

interface Props {
  project: ProjectSummary;
  frame: FrameState | null;
  marker: number;
  selected: string | null;
  /** Width of one frame in pixels. */
  zoom: number;
  onSeek: (marker: number) => void;
  onSelect: (id: string) => void;
}

/** Timeline list on the left, keyframe tracks on the right, as in the original's timeline tab. */
export function Timeline({ project, frame, marker, selected, zoom, onSeek, onSelect }: Props) {
  const tracks = useRef<HTMLDivElement>(null);
  const frames = Math.max(project.length + Math.ceil(project.tempo), MIN_FRAMES);
  const width = PADDING + frames * zoom;
  const frameX = (f: number) => PADDING + f * zoom;

  const seekFromPointer = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      const element = tracks.current;
      if (!element) return;
      const x = event.clientX - element.getBoundingClientRect().left + element.scrollLeft;
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

  // A label every second, or more often when zoomed in.
  const labelStep = zoom >= 12 ? Math.max(1, Math.round(project.tempo / 4)) : Math.max(1, Math.round(project.tempo));
  const labels: number[] = [];
  for (let f = 0; f <= frames; f += labelStep) labels.push(f);

  return (
    <div className="timeline">
      <div className="timeline-list">
        <div className="timeline-header" style={{ height: HEADER_HEIGHT }}>
          Timelines
        </div>
        {project.timelines.map((timeline, index) => {
          const state = frame?.timelines[index];
          const classes = ["timeline-row"];
          if (timeline.id === selected) classes.push("selected");
          if (timeline.hidden || state?.visible === false) classes.push("dimmed");
          return (
            <div
              key={timeline.id}
              className={classes.join(" ")}
              style={{ height: ROW_HEIGHT, paddingLeft: 8 + timeline.depth * 14 }}
              onClick={() => onSelect(timeline.id)}
              title={`${timeline.kind}${timeline.hidden ? " (hidden)" : ""}`}
            >
              <span className={`kind kind-${timeline.kind}`}>{timeline.kind}</span>
              <span className="name">{timeline.name || "(unnamed)"}</span>
              {frame?.activeCamera === timeline.id && <span className="badge">active</span>}
            </div>
          );
        })}
      </div>

      <div className="timeline-tracks" ref={tracks}>
        <div style={{ width, position: "relative" }}>
          <div
            className="timeline-ruler"
            style={{ height: HEADER_HEIGHT }}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
          >
            {labels.map((f) => (
              <span key={f} className="ruler-label" style={{ left: frameX(f) }}>
                {f}
              </span>
            ))}
          </div>

          {project.timelines.map((timeline) => (
            <div
              key={timeline.id}
              className={timeline.id === selected ? "track selected" : "track"}
              style={{ height: ROW_HEIGHT }}
              onClick={() => onSelect(timeline.id)}
            >
              {timeline.keyframes.map((position) => (
                <span
                  key={position}
                  className="keyframe"
                  style={{ left: frameX(position) }}
                  title={`Frame ${position}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    onSelect(timeline.id);
                    onSeek(position);
                  }}
                />
              ))}
            </div>
          ))}

          <div className="playhead" style={{ left: frameX(marker) }} />
        </div>
      </div>
    </div>
  );
}
