import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  appInfo,
  evaluateFrame,
  openProject,
  startupProject,
  type AppInfo,
  type FrameState,
  type ProjectSummary,
  type Vec3,
} from "./backend";
import { Timeline } from "./Timeline";

function formatTime(frame: number, tempo: number): string {
  if (tempo <= 0) return "0:00.00";
  const seconds = frame / tempo;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds - minutes * 60).toFixed(2).padStart(5, "0")}`;
}

const formatNumber = (value: number) => (Math.round(value * 1000) / 1000).toString();
const formatVec = (value: Vec3) => value.map(formatNumber).join(", ");

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [project, setProject] = useState<ProjectSummary | null>(null);
  const [frame, setFrame] = useState<FrameState | null>(null);
  const [marker, setMarker] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [zoom, setZoom] = useState(12);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const loadProject = useCallback(async (path: string) => {
    setBusy(true);
    setError(null);
    setPlaying(false);
    try {
      const summary = await openProject(path);
      setFrame(null);
      setSelected(null);
      setMarker(Math.min(summary.marker, summary.length));
      setProject(summary);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    appInfo().then(setInfo, (e) => setError(String(e)));
    // Opened by double-clicking a project file, or with a path argument.
    startupProject().then((path) => {
      if (path) void loadProject(path);
    }, () => undefined);
  }, [loadProject]);

  // The backend evaluates the scene; only the latest answer is shown so that
  // fast scrubbing cannot display a stale frame.
  const request = useRef(0);
  useEffect(() => {
    if (!project) return;
    const id = ++request.current;
    evaluateFrame(marker).then(
      (state) => {
        if (id === request.current) setFrame(state);
      },
      (e) => setError(String(e)),
    );
  }, [project, marker]);

  // Playback: advance the marker from the wall clock at the project tempo.
  useEffect(() => {
    if (!playing || !project) return;
    const startTime = performance.now();
    const startMarker = marker >= project.length ? 0 : marker;
    let handle = 0;
    const tick = () => {
      const next = startMarker + ((performance.now() - startTime) / 1000) * project.tempo;
      if (project.length > 0 && next >= project.length) {
        setMarker(project.length);
        setPlaying(false);
        return;
      }
      setMarker(next);
      handle = requestAnimationFrame(tick);
    };
    handle = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(handle);
    // The marker is only read when playback starts.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing, project]);

  const seek = useCallback((value: number) => {
    setPlaying(false);
    setMarker(value);
  }, []);

  const chooseProject = async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "Mine-imator project", extensions: ["miproject", "mproj", "mani"] }],
    });
    if (typeof path === "string") await loadProject(path);
  };

  const selectedIndex = project && selected ? project.timelines.findIndex((t) => t.id === selected) : -1;
  const selectedTimeline = project && selectedIndex >= 0 ? project.timelines[selectedIndex] : null;
  const selectedFrame = frame && selectedIndex >= 0 ? frame.timelines[selectedIndex] : null;

  return (
    <div className="app">
      <header className="toolbar">
        <span className="title">Mine-imator</span>
        <button onClick={chooseProject} disabled={busy}>
          {busy ? "Opening…" : "Open project…"}
        </button>
        {project && (
          <>
            <button className="secondary" onClick={() => seek(0)} title="Go to the first frame">
              ⏮
            </button>
            <button className="secondary" onClick={() => setPlaying(!playing)} disabled={project.length === 0}>
              {playing ? "Pause" : "Play"}
            </button>
            <span className="time">
              {formatTime(marker, project.tempo)} · frame {Math.floor(marker)} / {project.length}
            </span>
            <label className="zoom">
              Zoom
              <input type="range" min={2} max={32} value={zoom} onChange={(e) => setZoom(Number(e.target.value))} />
            </label>
          </>
        )}
        <span className="spacer" />
        {info && (
          <span className="muted">
            {info.version} · tracks {info.tracksVersion} · Minecraft {info.minecraftVersion}
          </span>
        )}
      </header>

      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}

      {!project && (
        <main className="empty">
          <p>Open a .miproject file to load its timelines and play back its animation data.</p>
          <p className="muted">
            The editor is being rebuilt system by system; see docs/PORTING_STATUS.md for what works today.
          </p>
        </main>
      )}

      {project && (
        <main className="workspace">
          <aside className="panel side">
            <h1>{project.name}</h1>
            {project.author && <p className="muted">by {project.author}</p>}
            {project.description && <p className="description">{project.description}</p>}
            <dl>
              <dt>Saved with</dt>
              <dd>
                {project.createdIn || "unknown"} (format {project.format})
              </dd>
              <dt>Video</dt>
              <dd>
                {project.videoWidth} × {project.videoHeight}, {project.tempo} fps
              </dd>
              <dt>Library</dt>
              <dd>
                {project.templates} templates, {project.resources} resources
              </dd>
              <dt>Scene</dt>
              <dd>
                {project.timelines.length} timelines, {project.cameras} cameras, {project.markers} markers
              </dd>
            </dl>

            {project.warnings.length > 0 && (
              <div className="warnings">
                <h2>Warnings</h2>
                <ul>
                  {project.warnings.map((warning, i) => (
                    <li key={i}>{warning}</li>
                  ))}
                </ul>
              </div>
            )}

            <h2 className="section">Selection</h2>
            {selectedTimeline && selectedFrame ? (
              <dl>
                <dt>Name</dt>
                <dd>{selectedTimeline.name || "(unnamed)"}</dd>
                <dt>Type</dt>
                <dd>{selectedTimeline.kind}</dd>
                <dt>Position</dt>
                <dd>{formatVec(selectedFrame.position)}</dd>
                <dt>Rotation</dt>
                <dd>{formatVec(selectedFrame.rotation)}</dd>
                <dt>Scale</dt>
                <dd>{formatVec(selectedFrame.scale)}</dd>
                <dt>In the world</dt>
                <dd>{formatVec(selectedFrame.worldPosition)}</dd>
                <dt>Visible</dt>
                <dd>{selectedFrame.visible ? "yes" : "no"}</dd>
                <dt>Alpha</dt>
                <dd>{formatNumber(selectedFrame.alpha)}</dd>
                <dt>Transition</dt>
                <dd>{selectedFrame.transition}</dd>
                <dt>Keyframes</dt>
                <dd>{selectedTimeline.keyframes.length}</dd>
              </dl>
            ) : (
              <p className="muted">Click a timeline to see its values at the current frame.</p>
            )}
          </aside>

          <section className="panel timeline-panel">
            {project.timelines.length === 0 ? (
              <p className="muted">This project has no timelines.</p>
            ) : (
              <Timeline
                project={project}
                frame={frame}
                marker={marker}
                selected={selected}
                zoom={zoom}
                onSeek={seek}
                onSelect={setSelected}
              />
            )}
          </section>
        </main>
      )}
    </div>
  );
}
