import { useEffect, useState } from "react";
import { forgetRecentProject, recentProjects, type RecentItem } from "./backend";

interface Props {
  version: string | null;
  busy: boolean;
  onBrowse: () => void;
  onOpen: (path: string) => void;
  onNew: () => void;
}

/** "Last opened 14.9.2026", as the original shows it for older entries. */
function lastOpened(item: RecentItem): string {
  if (item.lastOpened === null) return "Never opened";
  // The stored time is local wall-clock time, so it is read back without a
  // time zone conversion.
  const date = new Date(item.lastOpened);
  return `Last opened ${date.getUTCDate()}.${date.getUTCMonth() + 1}.${date.getUTCFullYear()}`;
}

/** The startup screen: logo and recent projects. */
export function StartScreen({ version, busy, onBrowse, onOpen, onNew }: Props) {
  const [items, setItems] = useState<RecentItem[] | null>(null);
  const [byName, setByName] = useState(false);

  useEffect(() => {
    recentProjects().then(setItems, () => setItems([]));
  }, []);

  const sorted = [...(items ?? [])].sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    return byName ? a.name.localeCompare(b.name) : (b.lastOpened ?? 0) - (a.lastOpened ?? 0);
  });

  return (
    <main className="start">
      <header className="start-header">
        <span className="logo">MINE-IMATOR</span>
        {version && <span className="logo-version">v {version}</span>}
      </header>

      <div className="start-body">
        <div className="start-actions">
          <span className="start-heading">Recent projects</span>
          <button className="ghost" onClick={() => setByName(!byName)} title="Change the order">
            Sort by {byName ? "name" : "date"}
          </button>
          <span className="spacer" />
          <button className="secondary" onClick={onBrowse} disabled={busy}>
            {busy ? "Opening…" : "Browse…"}
          </button>
          <button onClick={onNew} disabled={busy}>
            New project
          </button>
        </div>

        {items !== null && sorted.length === 0 && (
          <p className="muted">No recent projects. Use Browse… to open a .miproject file.</p>
        )}

        <div className="recent-grid">
          {sorted.map((item) => (
            <div
              key={item.filename}
              className={item.exists ? "recent-card" : "recent-card missing"}
              role="button"
              tabIndex={0}
              title={item.exists ? item.filename : `${item.filename} no longer exists`}
              onClick={() => item.exists && !busy && onOpen(item.filename)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && item.exists && !busy) onOpen(item.filename);
              }}
            >
              <div className="recent-thumbnail">
                {item.thumbnail ? <img src={item.thumbnail} alt="" /> : <span className="muted">No preview</span>}
                <button
                  className="recent-remove"
                  title="Remove from this list"
                  onClick={(event) => {
                    event.stopPropagation();
                    void forgetRecentProject(item.filename).then(setItems);
                  }}
                >
                  ✕
                </button>
              </div>
              <div className="recent-name">{item.name || "(unnamed)"}</div>
              <div className="recent-date">{item.exists ? lastOpened(item) : "File not found"}</div>
            </div>
          ))}
        </div>
      </div>
    </main>
  );
}
