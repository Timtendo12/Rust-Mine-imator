// Typed wrappers around the commands in `src-tauri/src/commands.rs`.
// Keep the shapes here in sync with the Rust structs.

import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  version: string;
  tracksVersion: string;
  minecraftVersion: string;
  projectFormat: number;
}

export interface TimelineSummary {
  id: string;
  name: string;
  kind: string;
  /** Number of ancestors in the timeline tree. */
  depth: number;
  /** Parent in the tree, null at the root. */
  parent: string | null;
  /** Position among the parent's children. */
  index: number;
  /** Part of a model or scenery; moves and is removed with its owner only. */
  part: boolean;
  /** Frames that have a keyframe, ascending. */
  keyframes: number[];
  hidden: boolean;
}

export interface EnvironmentSummary {
  /** Angle of the sun in degrees; 0 is noon, 15 degrees per hour. */
  skyTime: number;
  skyRotation: number;
  biome: string;
  skyColor: string;
  cloudsColor: string;
  sunlightColor: string;
  ambientColor: string;
  nightColor: string;
  twilight: boolean;
  cloudsShow: boolean;
  groundShow: boolean;
  fogShow: boolean;
  wind: boolean;
  textureAnimationSpeed: number;
}

export interface RecentItem {
  name: string;
  author: string;
  description: string;
  filename: string;
  /** Milliseconds since the Unix epoch, to be read as local wall-clock time. */
  lastOpened: number | null;
  pinned: boolean;
  exists: boolean;
  thumbnail: string | null;
}

export interface ProjectSummary {
  path: string | null;
  renderSettings: string;
  renderSamples: number;
  environment: EnvironmentSummary;
  name: string;
  author: string;
  description: string;
  createdIn: string;
  format: number;
  tempo: number;
  videoWidth: number;
  videoHeight: number;
  length: number;
  marker: number;
  templates: number;
  resources: number;
  markers: number;
  cameras: number;
  /** In tree order: every parent is followed by its children. */
  timelines: TimelineSummary[];
  warnings: string[];
  /** Unsaved changes. */
  changed: boolean;
  /** What undo and redo would do, if anything. */
  undo: string | null;
  redo: string | null;
}

export type Vec3 = [number, number, number];

export interface TimelineFrame {
  id: string;
  position: Vec3;
  rotation: Vec3;
  scale: Vec3;
  worldPosition: Vec3;
  visible: boolean;
  /** After multiplying with the parents' alpha. */
  alpha: number;
  /** The timeline's own alpha value. */
  alphaValue: number;
  transition: string;
}

export interface FrameState {
  marker: number;
  /** Same order as `ProjectSummary.timelines`. */
  timelines: TimelineFrame[];
  activeCamera: string | null;
}

export const appInfo = () => invoke<AppInfo>("app_info");

/** Path of the project file the application was started with. Only answered once. */
export const startupProject = () => invoke<string | null>("startup_project");

export const recentProjects = () => invoke<RecentItem[]>("recent_projects");

/** Removes a project from the recent list and returns the new list. */
export const forgetRecentProject = (filename: string) => invoke<RecentItem[]>("forget_recent_project", { filename });

export const closeProject = () => invoke<void>("close_project");

export const openProject = (path: string) => invoke<ProjectSummary>("open_project", { path });

export const evaluateFrame = (marker: number) => invoke<FrameState>("evaluate_frame", { marker });

export type ViewMode = "flat" | "shaded";

/** Where the viewport element is on the window, in physical pixels. */
export const setViewportRect = (x: number, y: number, width: number, height: number) =>
  invoke<void>("set_viewport_rect", { x, y, width, height });

export const viewportDrag = (kind: "orbit" | "pan", dx: number, dy: number) =>
  invoke<void>("viewport_drag", { kind, dx, dy });

/** Positive steps move the camera away. */
export const viewportZoom = (steps: number) => invoke<void>("viewport_zoom", { steps });

export const viewportResetCamera = () => invoke<void>("viewport_reset_camera");

export const setViewOptions = (mode: ViewMode, timelineCamera: boolean) =>
  invoke<void>("set_view_options", { mode, timelineCamera });

// Editing. Every edit answers with the project and the current frame as
// they are afterwards.

export interface Edited {
  project: ProjectSummary;
  frame: FrameState;
}

export interface KeyframeKey {
  timeline: string;
  position: number;
}

/** A number value by its name in project files, such as `POS_X`. */
export interface NumberEdit {
  name: string;
  value: number;
}

/**
 * Changes values of timelines at the current frame, adding a keyframe there
 * if needed. Edits with the same `merge` key in a row are one undo step and
 * each starts from the values before the first, so a drag sends its total
 * offset with mode "add".
 */
export const setTimelineValues = (timelines: string[], values: NumberEdit[], mode: "set" | "add", merge: string | null = null) =>
  invoke<Edited>("set_timeline_values", { timelines, values, mode, merge });

/** Ends a drag, so the next edit is an undo step of its own. */
export const finishEdit = () => invoke<void>("finish_edit");

/** Moves keyframes `offset` frames from where they were when the drag began. */
export const moveKeyframes = (keys: KeyframeKey[], offset: number, merge: string | null = null) =>
  invoke<Edited & { moved: KeyframeKey[] }>("move_keyframes", { keys, offset, merge });

export const removeKeyframes = (keys: KeyframeKey[]) => invoke<Edited>("remove_keyframes", { keys });

export const renameTimeline = (id: string, name: string) => invoke<Edited>("rename_timeline", { id, name });

export const setTimelinesHidden = (timelines: string[], hidden: boolean) =>
  invoke<Edited>("set_timelines_hidden", { timelines, hidden });

export const undo = () => invoke<Edited>("undo");

export const redo = () => invoke<Edited>("redo");

/** Saves to the project's file, or to `path` (save as). */
export const saveProject = (path: string | null = null) => invoke<Edited>("save_project", { path });

/** Timeline types the create menu offers, by their names in project files. */
export type CreatableKind = "folder" | "camera" | "pointlight" | "spotlight" | "cube" | "cone" | "cylinder" | "sphere" | "surface";

/** Adds a timeline at the end of the list; cameras start at the work camera. */
export const createTimeline = (kind: CreatableKind) => invoke<Edited & { created: string[] }>("create_timeline", { kind });

/** Removes timelines and everything below them. */
export const removeTimelines = (timelines: string[]) => invoke<Edited>("remove_timelines", { timelines });

export const duplicateTimelines = (timelines: string[]) =>
  invoke<Edited & { created: string[] }>("duplicate_timelines", { timelines });

/** Moves timelines under `parent` (the root for null), at `index` among its children or at the end. */
export const reparentTimelines = (timelines: string[], parent: string | null, index: number | null) =>
  invoke<Edited>("reparent_timelines", { timelines, parent, index });
