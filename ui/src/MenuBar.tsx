import { useEffect, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  /** Items without an action are shown disabled. */
  action?: () => void;
}

export interface Menu {
  title: string;
  items: MenuItem[];
}

/** The menu bar under the title bar: File, Edit, Render, View, Help. */
export function MenuBar({ menus }: { menus: Menu[] }) {
  const [open, setOpen] = useState<string | null>(null);
  const bar = useRef<HTMLElement>(null);

  // Close when clicking elsewhere or pressing Escape.
  useEffect(() => {
    if (open === null) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!bar.current?.contains(event.target as Node)) setOpen(null);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(null);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <nav className="menu-bar" ref={bar}>
      {menus.map((menu) => (
        <div key={menu.title} className="menu">
          <button
            className={open === menu.title ? "menu-title open" : "menu-title"}
            onClick={() => setOpen(open === menu.title ? null : menu.title)}
            onPointerEnter={() => open !== null && setOpen(menu.title)}
          >
            {menu.title}
          </button>
          {open === menu.title && (
            <div className="menu-list" role="menu">
              {menu.items.map((item) => (
                <button
                  key={item.label}
                  role="menuitem"
                  disabled={!item.action}
                  onClick={() => {
                    setOpen(null);
                    item.action?.();
                  }}
                >
                  {item.label}
                </button>
              ))}
            </div>
          )}
        </div>
      ))}
    </nav>
  );
}
