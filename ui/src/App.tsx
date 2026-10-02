import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { appInfo, inspectProject, type AppInfo, type ProjectSummary, type TimelineSummary } from "./backend";

const ROOT = "root";

interface TreeRow {
  timeline: TimelineSummary;
  depth: number;
}

/** Orders timelines as the timeline list shows them: children under their parent, by tree index. */
function buildTree(timelines: TimelineSummary[]): TreeRow[] {
  const ids = new Set(timelines.map((t) => t.id));
  const children = new Map<string, TimelineSummary[]>();
  for (const timeline of timelines) {
    const parent = ids.has(timeline.parent) ? timeline.parent : ROOT;
    const list = children.get(parent) ?? [];
    list.push(timeline);
    children.set(parent, list);
  }
  for (const list of children.values()) {
    list.sort((a, b) => (a.treeIndex ?? Number.MAX_SAFE_INTEGER) - (b.treeIndex ?? Number.MAX_SAFE_INTEGER));
  }

  const rows: TreeRow[] = [];
  const visited = new Set<string>();
  const visit = (parent: string, depth: number) => {
    for (const timeline of children.get(parent) ?? []) {
      if (visited.has(timeline.id)) continue;
      visited.add(timeline.id);
      rows.push({ timeline, depth });
      visit(timeline.id, depth + 1);
    }
  };
  visit(ROOT, 0);
  return rows;
}

function formatDuration(frames: number, tempo: number): string {
  if (tempo <= 0) return `${frames} frames`;
  const seconds = frames / tempo;
  const minutes = Math.floor(seconds / 60);
  const rest = (seconds - minutes * 60).toFixed(1).padStart(4, "0");
  return `${minutes}:${rest} (${frames} frames at ${tempo} fps)`;
}

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [project, setProject] = useState<ProjectSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    appInfo().then(setInfo, (e) => setError(String(e)));
  }, []);

  const rows = useMemo(() => (project ? buildTree(project.timelines) : []), [project]);

  const openProject = async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "Mine-imator project", extensions: ["miproject", "mproj", "mani"] }],
    });
    if (typeof path !== "string") return;

    setBusy(true);
    setError(null);
    try {
      setProject(await inspectProject(path));
    } catch (e) {
      setProject(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="app">
      <header className="toolbar">
        <span className="title">Mine-imator</span>
        <button onClick={openProject} disabled={busy}>
          {busy ? "Opening…" : "Open project…"}
        </button>
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

      {!project && !error && (
        <main className="empty">
          <p>Open a .miproject file to inspect it.</p>
          <p className="muted">
            The editor is being rebuilt system by system; see docs/PORTING_STATUS.md for what works today.
          </p>
        </main>
      )}

      {project && (
        <main className="content">
          <section className="panel">
            <h1>{project.name}</h1>
            {project.author && <p className="muted">by {project.author}</p>}
            {project.description && <p className="description">{project.description}</p>}
            <dl>
              <dt>File</dt>
              <dd className="path">{project.path}</dd>
              <dt>Saved with</dt>
              <dd>
                {project.createdIn || "unknown"} (format {project.format})
              </dd>
              <dt>Video</dt>
              <dd>
                {project.videoWidth} × {project.videoHeight}
              </dd>
              <dt>Length</dt>
              <dd>{formatDuration(project.length, project.tempo)}</dd>
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
          </section>

          <section className="panel tree">
            <h2>Timelines</h2>
            {rows.length === 0 && <p className="muted">This project has no timelines.</p>}
            <ul>
              {rows.map(({ timeline, depth }) => (
                <li
                  key={timeline.id}
                  className={timeline.hidden ? "hidden-timeline" : undefined}
                  style={{ paddingLeft: 8 + depth * 16 }}
                >
                  <span className="kind">{timeline.kind}</span>
                  <span className="name">{timeline.name || "(unnamed)"}</span>
                  <span className="muted">{timeline.keyframes} keyframes</span>
                </li>
              ))}
            </ul>
          </section>
        </main>
      )}
    </div>
  );
}
