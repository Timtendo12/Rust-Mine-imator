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
  /** Frames that have a keyframe, ascending. */
  keyframes: number[];
  hidden: boolean;
}

export interface ProjectSummary {
  path: string | null;
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
}

export type Vec3 = [number, number, number];

export interface TimelineFrame {
  id: string;
  position: Vec3;
  rotation: Vec3;
  scale: Vec3;
  worldPosition: Vec3;
  visible: boolean;
  alpha: number;
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
