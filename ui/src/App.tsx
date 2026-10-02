import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import {
  appInfo,
  closeProject,
  createBlock,
  createModel,
  createTimeline,
  duplicateTimelines,
  evaluateFrame,
  finishEdit,
  moveKeyframes,
  newProject,
  openProject,
  projectSettings,
  redo,
  removeKeyframes,
  removeTimelines,
  renameTimeline,
  reparentTimelines,
  saveProject,
  setProjectInfo,
  setSelection,
  setSetting,
  setTimelinesHidden,
  setTimelineValues,
  startupProject,
  timelineValues,
  undo,
  type AppInfo,
  type CreatableKind,
  type Edited,
  type FrameState,
  type KeyframeKey,
  type ValueEdit,
  type ValueGroup,
  type ProjectSummary,
  type Settings,
} from "./backend";
import { MenuBar, type Menu } from "./MenuBar";
import { Properties } from "./Properties";
import { StartScreen } from "./StartScreen";
import { keyframeId, Timeline } from "./Timeline";
import { Viewport } from "./Viewport";

/** Whether keys typed now belong to a text field rather than to shortcuts. */
const typingInField = () => {
  const element = document.activeElement;
  return element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement;
};

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [project, setProject] = useState<ProjectSummary | null>(null);
  const [frame, setFrame] = useState<FrameState | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [values, setValues] = useState<ValueGroup[]>([]);
  const [marker, setMarker] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [selectedKeyframes, setSelectedKeyframes] = useState<KeyframeKey[]>([]);
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
      setSelectedKeyframes([]);
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

  // Window title: "<project> - Mine-imator", with a star for unsaved changes.
  useEffect(() => {
    const title = project ? `${project.changed ? "*" : ""}${project.name || "Untitled"} - Mine-imator` : "Mine-imator";
    getCurrentWindow()
      .setTitle(title)
      .catch(() => undefined);
  }, [project]);

  // The frame editor shows the selected timeline at the current frame.
  useEffect(() => {
    if (!selected || !frame) {
      setValues([]);
      return;
    }
    let current = true;
    timelineValues(selected).then(
      (groups) => current && setValues(groups),
      () => current && setValues([]),
    );
    return () => {
      current = false;
    };
  }, [selected, frame]);

  // The viewport outlines the selection.
  const hasProject = project !== null;
  useEffect(() => {
    void setSelection(selected && hasProject ? [selected] : []);
  }, [selected, hasProject]);

  // Settings shown in the properties panel follow every change of the project.
  useEffect(() => {
    if (!project) return;
    projectSettings().then(setSettings, () => setSettings(null));
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

  /** Runs an edit and shows its result. Edits act on the whole frame the marker is in. */
  const run = useCallback(async (edit: () => Promise<Edited>) => {
    try {
      const result = await edit();
      setProject(result.project);
      setFrame(result.frame);
      return result;
    } catch (e) {
      setError(String(e));
      return null;
    }
  }, []);

  const editValues = useCallback(
    (values: ValueEdit[], mode: "set" | "add", merge: string | null) => {
      if (!selected) return;
      setPlaying(false);
      // Values are edited at a whole frame.
      setMarker((m) => Math.round(m));
      void run(() => setTimelineValues([selected], values, mode, merge));
    },
    [run, selected],
  );

  const moveSelected = useCallback(
    async (keys: KeyframeKey[], offset: number) => {
      const result = await run(() => moveKeyframes(keys, offset, "move-keyframes"));
      if (result && "moved" in result) {
        const moved = (result as Edited & { moved: KeyframeKey[] }).moved;
        setSelectedKeyframes(moved);
      }
    },
    [run],
  );

  const deleteSelectedKeyframes = useCallback(() => {
    if (selectedKeyframes.length === 0) return;
    const keys = selectedKeyframes;
    setSelectedKeyframes([]);
    void run(() => removeKeyframes(keys));
  }, [run, selectedKeyframes]);

  /** Delete: the selected keyframes if there are any, else the selected timeline. */
  const deleteSelection = useCallback(() => {
    if (selectedKeyframes.length > 0) {
      deleteSelectedKeyframes();
    } else if (selected) {
      const id = selected;
      setSelected(null);
      void run(() => removeTimelines([id]));
    }
  }, [deleteSelectedKeyframes, run, selected, selectedKeyframes]);

  const create = useCallback(
    async (make: () => Promise<Edited & { created: string[] }>) => {
      const result = await run(make);
      const created = (result as (Edited & { created: string[] }) | null)?.created ?? [];
      if (created.length > 0) {
        setSelected(created[0]);
        setSelectedKeyframes([]);
      }
    },
    [run],
  );

  const duplicate = useCallback(async () => {
    if (!selected) return;
    const id = selected;
    const result = await run(() => duplicateTimelines([id]));
    const created = (result as (Edited & { created: string[] }) | null)?.created ?? [];
    if (created.length > 0) {
      setSelected(created[0]);
      setSelectedKeyframes([]);
    }
  }, [run, selected]);

  const doUndo = useCallback(() => {
    setSelectedKeyframes([]);
    void run(undo);
  }, [run]);
  const doRedo = useCallback(() => {
    setSelectedKeyframes([]);
    void run(redo);
  }, [run]);

  const saveAs = useCallback(async () => {
    const path = await save({
      filters: [{ name: "Mine-imator project", extensions: ["miproject"] }],
      defaultPath: project?.path ?? `${project?.name || "Untitled"}.miproject`,
    });
    if (typeof path === "string") await run(() => saveProject(path));
  }, [project, run]);

  const doSave = useCallback(async () => {
    if (!project) return;
    if (project.path) await run(() => saveProject());
    else await saveAs();
  }, [project, run, saveAs]);

  /** Asks before throwing away unsaved changes. */
  const confirmDiscard = useCallback(async () => {
    if (!project?.changed) return true;
    return ask("The project has unsaved changes. Close it anyway?", { title: "Mine-imator", kind: "warning" });
  }, [project]);

  const browse = useCallback(async () => {
    if (!(await confirmDiscard())) return;
    const path = await open({
      multiple: false,
      filters: [{ name: "Mine-imator project", extensions: ["miproject", "mproj", "mani"] }],
    });
    if (typeof path === "string") await loadProject(path);
  }, [confirmDiscard, loadProject]);

  const startNew = useCallback(async () => {
    if (!(await confirmDiscard())) return;
    setPlaying(false);
    setFrame(null);
    setSelected(null);
    setSelectedKeyframes([]);
    setMarker(0);
    try {
      setProject(await newProject());
    } catch (e) {
      setError(String(e));
    }
  }, [confirmDiscard]);

  const close = useCallback(async () => {
    if (!(await confirmDiscard())) return;
    setPlaying(false);
    setProject(null);
    setFrame(null);
    setSelectedKeyframes([]);
    void closeProject();
  }, [confirmDiscard]);

  // Shortcuts. Text fields keep their own undo and Delete.
  useEffect(() => {
    if (!project) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const ctrl = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();
      if (ctrl && key === "s") {
        event.preventDefault();
        void (event.shiftKey ? saveAs() : doSave());
        return;
      }
      if (typingInField()) return;
      if (ctrl && key === "z" && !event.shiftKey) {
        event.preventDefault();
        doUndo();
      } else if (ctrl && (key === "y" || (key === "z" && event.shiftKey))) {
        event.preventDefault();
        doRedo();
      } else if (ctrl && key === "n") {
        event.preventDefault();
        void startNew();
      } else if (ctrl && key === "d") {
        event.preventDefault();
        void duplicate();
      } else if (event.key === "Delete") {
        deleteSelection();
      } else if (event.key === " ") {
        event.preventDefault();
        setPlaying((p) => !p);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [project, doSave, saveAs, doUndo, doRedo, deleteSelection, duplicate, startNew]);

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
        { label: "New project", action: () => void startNew(), shortcut: "Ctrl+N" },
        { label: "Open project…", action: browse },
        { label: "Save project", action: () => void doSave(), shortcut: "Ctrl+S" },
        { label: "Save as…", action: () => void saveAs(), shortcut: "Ctrl+Shift+S" },
        { label: "Import asset…" },
        { label: "Close project", action: () => void close() },
      ],
    },
    {
      title: "Edit",
      items: [
        { label: project.undo ? `Undo ${project.undo.toLowerCase()}` : "Undo", action: project.undo ? doUndo : undefined, shortcut: "Ctrl+Z" },
        { label: project.redo ? `Redo ${project.redo.toLowerCase()}` : "Redo", action: project.redo ? doRedo : undefined, shortcut: "Ctrl+Y" },
        { label: "Select all" },
        { label: "Duplicate timeline", action: selected ? () => void duplicate() : undefined, shortcut: "Ctrl+D" },
        {
          label: selectedKeyframes.length > 0 ? "Delete keyframes" : "Delete timeline",
          action: selectedKeyframes.length > 0 || selected ? deleteSelection : undefined,
          shortcut: "Delete",
        },
      ],
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
        <div className="error" role="alert" onClick={() => setError(null)}>
          {error}
        </div>
      )}
      <div className="editor">
        <div className="stage">
          <Viewport
            selected={selected}
            onPick={(id, keepSelection) => {
              if (id) {
                setSelected(id);
                setSelectedKeyframes([]);
              } else if (!keepSelection) {
                setSelected(null);
              }
            }}
          />
          <Timeline
            project={project}
            frame={frame}
            marker={marker}
            selected={selected}
            selectedKeyframes={new Set(selectedKeyframes.map(keyframeId))}
            playing={playing}
            onSeek={seek}
            onSelect={setSelected}
            onSelectKeyframes={setSelectedKeyframes}
            onMoveKeyframes={(keys, offset) => void moveSelected(keys, offset)}
            onMoveDone={() => void finishEdit()}
            onRename={(id, name) => void run(() => renameTimeline(id, name))}
            onToggleHidden={(id, hidden) => void run(() => setTimelinesHidden([id], hidden))}
            onCreate={(kind: CreatableKind) => void create(() => createTimeline(kind))}
            onCreateModel={(name) => void create(() => createModel(name))}
            onCreateBlock={(name) => void create(() => createBlock(name))}
            onReparent={(id, parent, index) => void run(() => reparentTimelines([id], parent, index))}
            onPlay={setPlaying}
          />
        </div>
        <Properties
          project={project}
          frame={frame}
          selected={selected}
          settings={settings}
          values={values}
          onSetSetting={(group, key, value, merge) => void run(() => setSetting(group, key, value, merge))}
          onSetInfo={(field, value) => void run(() => setProjectInfo(field, value))}
          onEditValues={editValues}
          onEditDone={() => void finishEdit()}
        />
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
        <span>
          <kbd>Ctrl</kbd> + <kbd>Z</kbd> Undo
        </span>
        <span>
          <kbd>Space</kbd> Play
        </span>
      </footer>
    </div>
  );
}
