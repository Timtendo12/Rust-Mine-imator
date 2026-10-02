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
  parent: string;
  treeIndex: number | null;
  keyframes: number;
  hidden: boolean;
}

export interface ProjectSummary {
  path: string;
  name: string;
  author: string;
  description: string;
  createdIn: string;
  format: number;
  tempo: number;
  videoWidth: number;
  videoHeight: number;
  length: number;
  templates: number;
  resources: number;
  markers: number;
  cameras: number;
  timelines: TimelineSummary[];
  warnings: string[];
}

export const appInfo = () => invoke<AppInfo>("app_info");

export const inspectProject = (path: string) => invoke<ProjectSummary>("inspect_project", { path });
