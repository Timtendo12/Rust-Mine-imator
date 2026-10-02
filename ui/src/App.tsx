import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import {
  appInfo,
  closeProject,
  evaluateFrame,
  openProject,
  startupProject,
  type AppInfo,
  type FrameState,
  type ProjectSummary,
} from "./backend";
import { MenuBar, type Menu } from "./MenuBar";
import { Properties } from "./Properties";
import { StartScreen } from "./StartScreen";
import { Timeline } from "./Timeline";
import { Viewport } from "./Viewport";

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [project, setProject] = useState<ProjectSummary | null>(null);
  const [frame, setFrame] = useState<FrameState | null>(null);
  const [marker, setMarker] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
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
    startupProject().then(
      (path) => {
        if (path) void loadProject(path);
      },
      () => undefined,
    );
  }, [loadProject]);

  // Window title: "<project> - Mine-imator".
  useEffect(() => {
    const title = project ? `${project.name || "Untitled"} - Mine-imator` : "Mine-imator";
    getCurrentWindow()
      .setTitle(title)
      .catch(() => undefined);
  }, [project]);

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
    setMarker(Math.max(0, value));
  }, []);

  const browse = useCallback(async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "Mine-imator project", extensions: ["miproject", "mproj", "mani"] }],
    });
    if (typeof path === "string") await loadProject(path);
  }, [loadProject]);

  const close = useCallback(() => {
    setPlaying(false);
    setProject(null);
    setFrame(null);
    void closeProject();
  }, []);

  if (!project) {
    return (
      <div className="app">
        {error && (
          <div className="error" role="alert">
            {error}
          </div>
        )}
        <StartScreen version={info?.tracksVersion ?? null} busy={busy} onBrowse={browse} onOpen={loadProject} />
      </div>
    );
  }

  // Items without an action are features that are not available yet.
  const menus: Menu[] = [
    {
      title: "File",
      items: [
        { label: "New project" },
        { label: "Open project…", action: browse },
        { label: "Save project" },
        { label: "Save as…" },
        { label: "Import asset…" },
        { label: "Close project", action: close },
      ],
    },
    {
      title: "Edit",
      items: [{ label: "Undo" }, { label: "Redo" }, { label: "Select all" }, { label: "Duplicate" }, { label: "Delete" }],
    },
    { title: "Render", items: [{ label: "Export image…" }, { label: "Export animation…" }] },
    {
      title: "View",
      items: [
        { label: "Go to first frame", action: () => seek(0) },
        { label: "Go to last frame", action: () => seek(project.length) },
      ],
    },
    { title: "Help", items: [{ label: `Version ${info?.version ?? ""} (tracks ${info?.tracksVersion ?? ""})` }] },
  ];

  return (
    <div className="app">
      <MenuBar menus={menus} />
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
      <div className="editor">
        <div className="stage">
          <Viewport />
          <Timeline
            project={project}
            frame={frame}
            marker={marker}
            selected={selected}
            playing={playing}
            onSeek={seek}
            onSelect={setSelected}
            onPlay={setPlaying}
          />
        </div>
        <Properties project={project} frame={frame} selected={selected} />
      </div>
      <footer className="shortcut-bar">
        <span>
          <kbd>Left click</kbd> Select timeline
        </span>
        <span>
          <kbd>Left drag</kbd> Orbit view
        </span>
        <span>
          <kbd>Shift</kbd> + <kbd>Left drag</kbd> Pan view
        </span>
        <span>
          <kbd>Wheel</kbd> Zoom
        </span>
      </footer>
    </div>
  );
}
