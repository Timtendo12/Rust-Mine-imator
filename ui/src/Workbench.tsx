import { useEffect, useRef, useState } from "react";
import { workbenchItems, type CreatableKind, type WorkbenchItems } from "./backend";

/** Timelines that need no settings, in the workbench's order. */
const BASIC: [CreatableKind, string][] = [
  ["folder", "Folder"],
  ["camera", "Camera"],
  ["pointlight", "Point light"],
  ["spotlight", "Spot light"],
  ["cube", "Cube"],
  ["cone", "Cone"],
  ["cylinder", "Cylinder"],
  ["sphere", "Sphere"],
  ["surface", "Surface"],
];

type Category = "basic" | "characters" | "specialBlocks" | "blocks" | "items";

const CATEGORIES: [Category, string][] = [
  ["basic", "Basic"],
  ["characters", "Character"],
  ["specialBlocks", "Special block"],
  ["blocks", "Block"],
  ["items", "Item"],
];

// The lists come from the asset pack and do not change while running.
let cachedItems: Promise<WorkbenchItems> | null = null;

/** Height of the workbench, to keep it on screen. */
const HEIGHT = 320;

interface Props {
  /** The button it opens from; it is placed next to it, above other panels. */
  anchor: DOMRect;
  onCreate: (kind: CreatableKind) => void;
  onCreateModel: (name: string) => void;
  onCreateBlock: (name: string) => void;
  onCreateItem: (name: string) => void;
  onClose: () => void;
}

/** The workbench: picks something to add to the scene. */
export function Workbench({ anchor, onCreate, onCreateModel, onCreateBlock, onCreateItem, onClose }: Props) {
  const [category, setCategory] = useState<Category>("basic");
  const [items, setItems] = useState<WorkbenchItems | null>(null);
  const [search, setSearch] = useState("");
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    cachedItems ??= workbenchItems();
    cachedItems.then(setItems, () => (cachedItems = null));
  }, []);

  // Close on a click elsewhere or Escape.
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as HTMLElement;
      if (!panel.current?.contains(target) && !target.closest(".create-button")) onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);

  const query = search.trim().toLowerCase();
  const pick = (action: () => void) => {
    action();
    onClose();
  };

  let entries: { key: string; label: string; action: () => void }[];
  if (category === "basic") {
    entries = BASIC.map(([kind, label]) => ({ key: kind, label, action: () => onCreate(kind) }));
  } else {
    const list = items?.[category] ?? [];
    const create = category === "blocks" ? onCreateBlock : category === "items" ? onCreateItem : onCreateModel;
    entries = list.map((item) => ({ key: item.name, label: item.label, action: () => create(item.name) }));
  }
  const shown = entries.filter((e) => !query || e.label.toLowerCase().includes(query));

  return (
    <div
      className="workbench"
      ref={panel}
      role="dialog"
      aria-label="Workbench"
      style={{
        // Below the button if it fits, else above it.
        top: anchor.bottom + 2 + HEIGHT <= window.innerHeight ? anchor.bottom + 2 : Math.max(0, anchor.top - HEIGHT - 2),
        left: anchor.left,
      }}
    >
      <div className="workbench-categories">
        {CATEGORIES.map(([key, label]) => (
          <button
            key={key}
            className={key === category ? "active" : ""}
            onClick={() => {
              setCategory(key);
              setSearch("");
            }}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="workbench-items">
        {category !== "basic" && (
          <input autoFocus type="search" placeholder="Search…" value={search} onChange={(e) => setSearch(e.target.value)} />
        )}
        <div className="workbench-list">
          {category !== "basic" && !items && <p className="muted">Loading…</p>}
          {shown.map((entry) => (
            <button key={entry.key} onClick={() => pick(entry.action)}>
              {entry.label}
            </button>
          ))}
          {items && shown.length === 0 && <p className="muted">Nothing found.</p>}
        </div>
      </div>
    </div>
  );
}
