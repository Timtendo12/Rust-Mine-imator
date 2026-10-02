import { useEffect, useRef, useState, type PointerEvent } from "react";
import { setViewOptions, setViewportRect, viewportDrag, viewportResetCamera, viewportZoom, type ViewMode } from "./backend";

/**
 * The 3D view. The scene itself is drawn by the backend straight onto the
 * window, underneath the webview; this element is a transparent hole that
 * tells the backend where to draw and forwards mouse input.
 */
export function Viewport() {
  const element = useRef<HTMLDivElement>(null);
  const [mode, setMode] = useState<ViewMode>("shaded");
  const [timelineCamera, setTimelineCamera] = useState(false);

  // Keep the backend informed of where the hole is.
  useEffect(() => {
    const target = element.current;
    if (!target) return;
    const report = () => {
      const rect = target.getBoundingClientRect();
      const scale = window.devicePixelRatio;
      void setViewportRect(rect.left * scale, rect.top * scale, rect.width * scale, rect.height * scale);
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
  }, []);

  useEffect(() => {
    void setViewOptions(mode, timelineCamera);
  }, [mode, timelineCamera]);

  // Mouse movement is collected and sent once per animation frame.
  const pending = useRef({ dx: 0, dy: 0, pan: false, scheduled: false });
  const flush = () => {
    const drag = pending.current;
    drag.scheduled = false;
    if (drag.dx !== 0 || drag.dy !== 0) {
      void viewportDrag(drag.pan ? "pan" : "orbit", drag.dx, drag.dy);
      drag.dx = 0;
      drag.dy = 0;
    }
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 && event.button !== 1) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    // Middle button or Shift pans; plain left button orbits.
    pending.current.pan = event.button === 1 || event.shiftKey;
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
    const drag = pending.current;
    drag.dx += event.movementX;
    drag.dy += event.movementY;
    if (!drag.scheduled) {
      drag.scheduled = true;
      requestAnimationFrame(flush);
    }
  };

  // React registers wheel listeners as passive, which cannot prevent the
  // page from scrolling, so this one is attached by hand.
  useEffect(() => {
    const target = element.current;
    if (!target) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      void viewportZoom(Math.sign(event.deltaY));
    };
    target.addEventListener("wheel", onWheel, { passive: false });
    return () => target.removeEventListener("wheel", onWheel);
  }, []);

  return (
    <div className="viewport-cell">
      <div className="viewport-bar">
        <label>
          View
          <select value={mode} onChange={(e) => setMode(e.target.value as ViewMode)}>
            <option value="flat">Flat</option>
            <option value="shaded">Shaded</option>
          </select>
        </label>
        <label>
          Camera
          <select value={timelineCamera ? "active" : "work"} onChange={(e) => setTimelineCamera(e.target.value === "active")}>
            <option value="work">Work camera</option>
            <option value="active">Active camera</option>
          </select>
        </label>
        <button className="secondary" onClick={() => void viewportResetCamera()} disabled={timelineCamera}>
          Reset view
        </button>
        <span className="spacer" />
        <span className="muted">Drag to orbit · Shift or middle drag to pan · wheel to zoom</span>
      </div>
      <div
        className="viewport"
        ref={element}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onContextMenu={(event) => event.preventDefault()}
      />
    </div>
  );
}
