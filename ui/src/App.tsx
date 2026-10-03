import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import {
  addMarker,
  appInfo,
  audioPlay,
  audioStop,
  backupProject,
  closeProject,
  copyKeyframes,
  createAudio,
  createBlock,
  createItem,
  createScenery,
  createText,
  cycleRepeat,
  editMarker,
  createKeyframes,
  createModel,
  createParticles,
  createTimeline,
  duplicateTimelines,
  evaluateFrame,
  exportImage,
  finishEdit,
  lastBackup,
  moveKeyframes,
  newProject,
  openProject,
  pasteKeyframes,
  projectSettings,
  redo,
  removeKeyframes,
  removeMarker,
  removeTimelines,
  renameTimeline,
  reparentTimelines,
  saveProject,
  setModelSkin,
  setProjectInfo,
  setRegion,
  setSelection,
  setTimelineSetting,
  setSetting,
  setTimelinesHidden,
  setTimelineValues,
  startupProject,
  timelineSettings,
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
  type TimelineSettings,
} from "./backend";
import { ExportDialog } from "./ExportDialog";
import { MenuBar, type Menu } from "./MenuBar";
import { Properties } from "./Properties";
import { StartScreen } from "./StartScreen";
import { keyframeId, Timeline, type SelectMode } from "./Timeline";
import { Viewport } from "./Viewport";

/** Minutes between backups of a changed project (`setting_backup_time`). */
const BACKUP_MINUTES = 3;

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
  const [tlSettings, setTlSettings] = useState<TimelineSettings | null>(null);
  const [marker, setMarker] = useState(0);
  // Selected timelines; the last one is the one the editors show.
  const [selection, setSelectedIds] = useState<string[]>([]);
  const selected = selection.length > 0 ? selection[selection.length - 1] : null;
  const setSelected = useCallback((id: string | null) => setSelectedIds(id ? [id] : []), []);
  const [selectedKeyframes, setSelectedKeyframes] = useState<KeyframeKey[]>([]);
  // How many keyframes were copied, and the frame under the mouse in the timeline.
  const [copied, setCopied] = useState(0);
  const hoverFrame = useRef<number | null>(null);
  const [playing, setPlaying] = useState(false);
  const [exporting, setExporting] = useState(false);
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

  // Backups: every few minutes, if the project has a file and changed.
  const projectPath = project?.path ?? null;
  useEffect(() => {
    if (!projectPath) return;
    const timer = setInterval(() => {
      backupProject().catch((e) => setError(`The backup failed: ${e}`));
    }, BACKUP_MINUTES * 60_000);
    return () => clearInterval(timer);
  }, [projectPath]);

  // The frame editor shows the selected timeline at the current frame.
  useEffect(() => {
    if (!selected || !frame) {
      setValues([]);
      setTlSettings(null);
      return;
    }
    let current = true;
    timelineValues(selected).then(
      (groups) => current && setValues(groups),
      () => current && setValues([]),
    );
    timelineSettings(selected).then(
      (settings) => current && setTlSettings(settings),
      () => current && setTlSettings(null),
    );
    return () => {
      current = false;
    };
  }, [selected, frame]);

  // The viewport outlines the selection.
  const hasProject = project !== null;
  useEffect(() => {
    void setSelection(hasProject ? selection : []);
  }, [selection, hasProject]);

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
    // With a repeat mode the region, or else the whole animation, loops
    // (`app_update_play`); without one playback stops at the end.
    const [loopStart, loopEnd] = project.region ?? [0, project.length];
    const repeating = project.repeat !== "none" && loopEnd > loopStart;
    let startTime = performance.now();
    let startMarker = marker >= (repeating ? loopEnd : project.length) ? (repeating ? loopStart : 0) : marker;
    let handle = 0;
    void audioPlay(startMarker);
    const tick = () => {
      let next = startMarker + ((performance.now() - startTime) / 1000) * project.tempo;
      if (repeating && next >= loopEnd) {
        startTime = performance.now();
        startMarker = loopStart;
        next = loopStart;
        void audioPlay(loopStart);
      } else if (!repeating && project.length > 0 && next >= project.length) {
        setMarker(project.length);
        setPlaying(false);
        return;
      }
      setMarker(next);
      handle = requestAnimationFrame(tick);
    };
    handle = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(handle);
      void audioStop();
    };
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
      if (selection.length === 0) return;
      setPlaying(false);
      // Values are edited at a whole frame.
      setMarker((m) => Math.round(m));
      void run(() => setTimelineValues(selection, selectedKeyframes, values, mode, merge));
    },
    [run, selection, selectedKeyframes],
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

  /** A keyframe at the marker for the selected timelines that have none there. */
  const createKeyframe = useCallback(async () => {
    if (selection.length === 0) return;
    setPlaying(false);
    setMarker((m) => Math.round(m));
    const result = await run(() => createKeyframes(selection));
    const keys = (result as (Edited & { keys: KeyframeKey[] }) | null)?.keys ?? [];
    if (keys.length > 0) setSelectedKeyframes(keys);
  }, [run, selection]);

  const copySelectedKeyframes = useCallback(
    (cut: boolean) => {
      if (selectedKeyframes.length === 0) return;
      const keys = selectedKeyframes;
      setCopied(keys.length);
      if (cut) setSelectedKeyframes([]);
      void run(() => copyKeyframes(keys, cut));
    },
    [run, selectedKeyframes],
  );

  /** Pastes at the frame under the mouse in the timeline, else at the marker. */
  const paste = useCallback(async () => {
    if (copied === 0) return;
    const position = hoverFrame.current ?? Math.round(marker);
    const result = await run(() => pasteKeyframes(position, selection));
    const keys = (result as (Edited & { keys: KeyframeKey[] }) | null)?.keys ?? [];
    if (keys.length > 0) setSelectedKeyframes(keys);
  }, [copied, marker, run, selection]);

  /** Delete: the selected keyframes if there are any, else the selected timelines. */
  const deleteSelection = useCallback(() => {
    if (selectedKeyframes.length > 0) {
      deleteSelectedKeyframes();
    } else if (selection.length > 0) {
      const ids = selection;
      setSelectedIds([]);
      void run(() => removeTimelines(ids));
    }
  }, [deleteSelectedKeyframes, run, selection, selectedKeyframes]);

  const create = useCallback(
    async (make: () => Promise<Edited & { created: string[] }>) => {
      const result = await run(make);
      const created = (result as (Edited & { created: string[] }) | null)?.created ?? [];
      if (created.length > 0) {
        setSelected(created[0]);
        setSelectedKeyframes([]);
      }
    },
    [run, setSelected],
  );

  /** Adds a particle spawner from a preset, or (null) from a file that is asked for. */
  const addParticles = useCallback(
    async (preset: string | null) => {
      if (preset !== null) {
        await create(() => createParticles(preset));
        return;
      }
      const path = await open({ multiple: false, filters: [{ name: "Particles", extensions: ["miparticles"] }] });
      if (typeof path === "string") await create(() => createParticles(null, path));
    },
    [create],
  );

  /** Asks for a sound file and adds it at the playhead. */
  const addAudio = useCallback(async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "Sound", extensions: ["mp3", "ogg", "wav", "flac", "m4a"] }],
    });
    if (typeof path === "string") await create(() => createAudio(path, selection));
  }, [create, selection]);

  /** Asks for a schematic or structure file and adds it as scenery. */
  const addScenery = useCallback(async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "Scenery (schematics, structures)", extensions: ["schematic", "schem", "nbt", "blocks"] }],
    });
    if (typeof path === "string") await create(() => createScenery(path));
  }, [create]);

  /** Asks for an image and makes it the skin of the selected characters; `false` restores the default. */
  const changeSkin = useCallback(
    async (browse: boolean) => {
      if (selection.length === 0) return;
      let path: string | null = null;
      if (browse) {
        const picked = await open({ multiple: false, filters: [{ name: "Skin image", extensions: ["png"] }] });
        if (typeof picked !== "string") return;
        path = picked;
      }
      await run(() => setModelSkin(selection, path));
    },
    [run, selection],
  );

  const duplicate = useCallback(async () => {
    if (selection.length === 0) return;
    const ids = selection;
    const result = await run(() => duplicateTimelines(ids));
    const created = (result as (Edited & { created: string[] }) | null)?.created ?? [];
    if (created.length > 0) {
      setSelectedIds(created);
      setSelectedKeyframes([]);
    }
  }, [run, selection]);

  /** Selects a timeline: alone, added to or taken out of the selection, or with all rows up to it. */
  const select = useCallback(
    (id: string, mode: SelectMode) => {
      setSelectedIds((current) => {
        if (mode === "toggle") return current.includes(id) ? current.filter((s) => s !== id) : [...current, id];
        if (mode === "add") return [...current.filter((s) => s !== id), id];
        const order = project?.timelines.map((t) => t.id) ?? [];
        const from = order.indexOf(current[current.length - 1]);
        const to = order.indexOf(id);
        if (mode === "range" && from >= 0 && to >= 0) {
          const range = from < to ? order.slice(from, to + 1) : order.slice(to, from + 1).reverse();
          return [...current.filter((s) => !range.includes(s)), ...range];
        }
        return [id];
      });
    },
    [project],
  );

  const selectAll = useCallback(() => {
    setSelectedIds(project?.timelines.map((t) => t.id) ?? []);
  }, [project]);

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

  const doExportImage = useCallback(async () => {
    const path = await save({
      filters: [{ name: "PNG image", extensions: ["png"] }],
      defaultPath: `${project?.name || "Untitled"}.png`,
    });
    if (typeof path !== "string") return;
    try {
      await exportImage(path);
    } catch (e) {
      setError(String(e));
    }
  }, [project]);

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

  const openLastBackup = useCallback(async () => {
    const path = await lastBackup();
    if (!path) {
      setError("This project has no backup yet.");
      return;
    }
    if (await confirmDiscard()) await loadProject(path);
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
      if (event.key === "F10") {
        event.preventDefault();
        void doExportImage();
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
      } else if (ctrl && key === "a") {
        event.preventDefault();
        selectAll();
      } else if (ctrl && key === "d") {
        event.preventDefault();
        void duplicate();
      } else if (ctrl && key === "q") {
        event.preventDefault();
        void createKeyframe();
      } else if (ctrl && key === "c") {
        copySelectedKeyframes(false);
      } else if (ctrl && key === "x") {
        copySelectedKeyframes(true);
      } else if (ctrl && key === "v") {
        void paste();
      } else if (event.key === "Delete") {
        deleteSelection();
      } else if (event.key === " ") {
        event.preventDefault();
        setPlaying((p) => !p);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [project, doSave, saveAs, doUndo, doRedo, deleteSelection, duplicate, startNew, doExportImage, createKeyframe, copySelectedKeyframes, paste, selectAll]);

  if (!project) {
    return (
      <div className="app">
        {error && (
          <div className="error" role="alert">
            {error}
          </div>
        )}
        <StartScreen version={info?.tracksVersion ?? null} busy={busy} onBrowse={browse} onOpen={loadProject} onNew={() => void startNew()} />
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
        { label: "Open last backup", action: project.path ? () => void openLastBackup() : undefined },
        { label: "Import asset…" },
        { label: "Close project", action: () => void close() },
      ],
    },
    {
      title: "Edit",
      items: [
        { label: project.undo ? `Undo ${project.undo.toLowerCase()}` : "Undo", action: project.undo ? doUndo : undefined, shortcut: "Ctrl+Z" },
        { label: project.redo ? `Redo ${project.redo.toLowerCase()}` : "Redo", action: project.redo ? doRedo : undefined, shortcut: "Ctrl+Y" },
        { label: "Select all", action: selectAll, shortcut: "Ctrl+A" },
        { label: "Add marker", action: () => void run(addMarker) },
        { label: "Create keyframe", action: selected ? () => void createKeyframe() : undefined, shortcut: "Ctrl+Q" },
        { label: "Copy keyframes", action: selectedKeyframes.length > 0 ? () => copySelectedKeyframes(false) : undefined, shortcut: "Ctrl+C" },
        { label: "Cut keyframes", action: selectedKeyframes.length > 0 ? () => copySelectedKeyframes(true) : undefined, shortcut: "Ctrl+X" },
        { label: "Paste keyframes", action: copied > 0 ? () => void paste() : undefined, shortcut: "Ctrl+V" },
        { label: selection.length > 1 ? "Duplicate timelines" : "Duplicate timeline", action: selected ? () => void duplicate() : undefined, shortcut: "Ctrl+D" },
        {
          label: selectedKeyframes.length > 0 ? "Delete keyframes" : selection.length > 1 ? "Delete timelines" : "Delete timeline",
          action: selectedKeyframes.length > 0 || selected ? deleteSelection : undefined,
          shortcut: "Delete",
        },
      ],
    },
    {
      title: "Render",
      items: [
        { label: "Export image…", action: () => void doExportImage(), shortcut: "F10" },
        {
          label: "Export animation…",
          action: () => {
            setPlaying(false);
            setExporting(true);
          },
        },
      ],
    },
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
            frame={frame}
            onEditValues={editValues}
            onEditDone={() => void finishEdit()}
            onPick={(id, keepSelection) => {
              if (id) {
                select(id, keepSelection ? "toggle" : "only");
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
            selection={selection}
            selectedKeyframes={new Set(selectedKeyframes.map(keyframeId))}
            playing={playing}
            onSeek={seek}
            onHoverFrame={(f) => (hoverFrame.current = f)}
            onSetRegion={(start, end) => void run(() => setRegion(start, end))}
            onCycleRepeat={() => void run(cycleRepeat)}
            onAddMarker={() => void run(addMarker)}
            onEditMarker={(id, change, merge) => void run(() => editMarker(id, change, merge))}
            onRemoveMarker={(id) => void run(() => removeMarker(id))}
            onSelect={select}
            onSelectKeyframes={setSelectedKeyframes}
            onMoveKeyframes={(keys, offset) => void moveSelected(keys, offset)}
            onMoveDone={() => void finishEdit()}
            onRename={(id, name) => void run(() => renameTimeline(id, name))}
            onToggleHidden={(id, hidden) => void run(() => setTimelinesHidden([id], hidden))}
            onCreate={(kind: CreatableKind) => void create(() => createTimeline(kind))}
            onCreateModel={(name) => void create(() => createModel(name))}
            onCreateBlock={(name) => void create(() => createBlock(name))}
            onCreateItem={(name) => void create(() => createItem(name))}
            onCreateText={() => void create(createText)}
            onCreateScenery={() => void addScenery()}
            onCreateAudio={() => void addAudio()}
            onCreateParticles={(preset) => void addParticles(preset)}
            onReparent={(ids, parent, index) => void run(() => reparentTimelines(ids, parent, index))}
            onPlay={setPlaying}
          />
        </div>
        <Properties
          project={project}
          frame={frame}
          selected={selected}
          settings={settings}
          values={values}
          timelineSettings={tlSettings}
          onChangeSkin={(browse) => void changeSkin(browse)}
          onSetTimelineSetting={(group, key, value) =>
            selection.length > 0 && void run(() => setTimelineSetting(selection, group, key, value))
          }
          onSetSetting={(group, key, value, merge) => void run(() => setSetting(group, key, value, merge))}
          onSetInfo={(field, value) => void run(() => setProjectInfo(field, value))}
          onEditValues={editValues}
          onEditDone={() => void finishEdit()}
        />
      </div>
      {exporting && <ExportDialog project={project} onClose={() => setExporting(false)} onError={setError} />}
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
