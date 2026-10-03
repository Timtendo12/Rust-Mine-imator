import { useRef, useState, type PointerEvent, type ReactNode } from "react";
import type {
  FrameState,
  ProjectSummary,
  Settings,
  TimelineSettingGroup,
  TimelineSettings,
  ValueEdit,
  ValueEntry,
  ValueGroup,
  Vec3,
} from "./backend";

/** Switches of the timeline settings with their labels, by group. */
const TIMELINE_SWITCHES: [TimelineSettingGroup, string, [string, string][]][] = [
  [
    "inherit",
    "Inherit from parent",
    [
      ["position", "Position"],
      ["rotation", "Rotation"],
      ["scale", "Scale"],
      ["alpha", "Alpha"],
      ["color", "Color"],
      ["texture", "Texture"],
      ["surface", "Surface"],
      ["subsurface", "Subsurface"],
      ["visibility", "Visibility"],
      ["bend", "Bend"],
      ["rot_point", "Rotation point"],
      ["glow_color", "Glow color"],
      ["select", "Selection"],
      ["pose", "Pose"],
    ],
  ],
  [
    "appearance",
    "Appearance",
    [
      ["backfaces", "Show backfaces"],
      ["texture_blur", "Blur texture"],
      ["texture_filtering", "Texture filtering"],
      ["shadows", "Cast shadows"],
      ["ssao", "Ambient occlusion"],
      ["glow", "Glow"],
      ["glow_texture", "Glow uses texture"],
      ["only_render_glow", "Only render glow"],
      ["fog", "Affected by fog"],
    ],
  ],
  [
    "flags",
    "Timeline",
    [
      ["lock", "Locked (cannot be clicked)"],
      ["lq_hiding", "Hide in low quality"],
      ["hq_hiding", "Hide in high quality"],
      ["scale_resize", "Scale resizes children"],
      ["lock_bend", "Follow the bent half"],
      ["wind", "Wind"],
      ["wind_terrain", "Wind on terrain"],
    ],
  ],
];

/** Groups of the frame editor that start opened. */
const OPEN_GROUPS = new Set(["Position", "Rotation", "Scale", "Bend", "Color", "Light", "Camera", "Keyframe"]);

const formatNumber = (value: number) => (Math.round(value * 1000) / 1000).toString();
const formatVec = (value: Vec3) => value.map(formatNumber).join(", ");

/** Sun angle to clock time: 0 degrees is noon and the sun moves 15 degrees per hour. */
function clockTime(skyTime: number): string {
  const minutes = Math.round((((12 + skyTime / 15) % 24) + 24) % 24 * 60) % 1440;
  return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}`;
}

function projectFolder(path: string | null): string {
  if (!path) return "Not saved yet";
  const parts = path.replace(/\\/g, "/").split("/");
  return parts.slice(0, -1).join("/");
}

function Section({ title, open, onToggle, children }: { title: string; open: boolean; onToggle: () => void; children: ReactNode }) {
  return (
    <section className={open ? "section open" : "section"}>
      <button className="section-header" onClick={onToggle} aria-expanded={open}>
        <span>{title}</span>
        <span className="chevron">{open ? "▾" : "▸"}</span>
      </button>
      {open && <div className="section-body">{children}</div>}
    </section>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="field">
      <div className="field-label">{label}</div>
      <div className="field-value">{children}</div>
    </div>
  );
}

/** A colour picker; `onPick` is called while the picker moves. */
function Swatch({ label, color, onPick }: { label: string; color: string; onPick: (color: string) => void }) {
  return (
    <label className="swatch">
      <div className="field-label">{label}</div>
      <input className="swatch-color" type="color" value={color.toLowerCase()} onChange={(e) => onPick(e.target.value.toUpperCase())} />
    </label>
  );
}

function Toggle({ label, on, onChange }: { label: string; on: boolean; onChange: (on: boolean) => void }) {
  return (
    <button className="toggle-row" onClick={() => onChange(!on)} aria-pressed={on}>
      <span>{label}</span>
      <span className={on ? "switch on" : "switch"} />
    </button>
  );
}

/** Text that is committed when the field is left or Enter is pressed. */
function TextInput({ value, multiline, onCommit }: { value: string; multiline?: boolean; onCommit: (value: string) => void }) {
  const [text, setText] = useState<string | null>(null);
  const commit = () => {
    if (text !== null && text !== value) onCommit(text);
    setText(null);
  };
  const common = {
    className: "text-input",
    value: text ?? value,
    onChange: (e: { target: { value: string } }) => setText(e.target.value),
    onBlur: commit,
  };
  return multiline ? (
    <textarea {...common} rows={3} />
  ) : (
    <input {...common} onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
  );
}

/**
 * A number that can be typed, or changed by dragging its label sideways
 * (`step` per pixel). Dragging reports the total change since it began.
 */
function NumberInput({
  label,
  value,
  step,
  onSet,
  onDrag,
  onDragEnd,
}: {
  label: string;
  value: number;
  step: number;
  onSet: (value: number) => void;
  /** Total change since the drag began, and the value it began at. */
  onDrag: (offset: number, startValue: number) => void;
  onDragEnd: () => void;
}) {
  // While the field has focus it shows what is typed; otherwise the value.
  const [text, setText] = useState<string | null>(null);
  const start = useRef<number | null>(null);
  const startValue = useRef(value);

  const commit = () => {
    if (text !== null) {
      const parsed = Number(text.replace(",", "."));
      if (Number.isFinite(parsed) && formatNumber(parsed) !== formatNumber(value)) onSet(parsed);
    }
    setText(null);
  };

  return (
    // Not a <label>: clicking the drag handle must not focus the field, or
    // the field would stop showing the value being dragged.
    <div className="number-input">
      <span
        className="number-label"
        title="Drag to change"
        onPointerDown={(e: PointerEvent<HTMLSpanElement>) => {
          e.preventDefault();
          e.currentTarget.setPointerCapture(e.pointerId);
          start.current = e.clientX;
          startValue.current = value;
        }}
        onPointerMove={(e) => {
          if (start.current === null) return;
          const offset = Math.round((e.clientX - start.current) * step * 1000) / 1000;
          if (offset !== 0) onDrag(offset, startValue.current);
        }}
        onPointerUp={() => {
          if (start.current !== null) onDragEnd();
          start.current = null;
        }}
      >
        {label}
      </span>
      <input
        value={text ?? formatNumber(value)}
        inputMode="decimal"
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            // Throw away what was typed.
            setText(null);
            const input = e.currentTarget;
            requestAnimationFrame(() => input.blur());
          }
        }}
      />
    </div>
  );
}

interface Props {
  project: ProjectSummary;
  frame: FrameState | null;
  selected: string | null;
  /** Background and render settings as files store them. */
  settings: Settings | null;
  /** Changes a background or render setting; `merge` joins a drag into one undo step. */
  onSetSetting: (group: "background" | "render", key: string, value: unknown, merge: string | null) => void;
  onSetInfo: (field: "name" | "author" | "description" | "tempo" | "video_size", value: unknown) => void;
  /** Changes values of the selected timeline at the current frame. */
  /** The frame editor of the selected timeline. */
  values: ValueGroup[];
  /** Settings of the selected timeline that are not animated. */
  timelineSettings: TimelineSettings | null;
  onSetTimelineSetting: (group: TimelineSettingGroup, key: string, value: unknown) => void;
  /** Picks a skin for the selected characters (true), or restores the default one (false). */
  onChangeSkin: (browse: boolean) => void;
  onEditValues: (values: ValueEdit[], mode: "set" | "add", merge: string | null) => void;
  onEditDone: () => void;
}

/** The properties panel on the right. */
export function Properties(props: Props) {
  const { project, frame, selected, settings, values, onEditValues, onEditDone, onSetSetting, onSetInfo } = props;
  const { timelineSettings, onSetTimelineSetting, onChangeSkin } = props;
  const [open, setOpen] = useState<Record<string, boolean>>({ project: true, selection: true });
  const toggle = (key: string) => setOpen((current) => ({ ...current, [key]: !current[key] }));
  // Which groups of the frame editor are open; kept while values refresh.
  const [openGroups, setOpenGroups] = useState(OPEN_GROUPS);

  const index = selected ? project.timelines.findIndex((t) => t.id === selected) : -1;
  const timeline = index >= 0 ? project.timelines[index] : null;
  const state = frame && index >= 0 ? frame.timelines[index] : null;
  const environment = project.environment;
  const background = settings?.background ?? {};
  const render = settings?.render ?? {};
  const bool = (map: Record<string, unknown>, key: string) => map[key] === true;
  const num = (map: Record<string, unknown>, key: string) => (typeof map[key] === "number" ? (map[key] as number) : 0);
  const text = (map: Record<string, unknown>, key: string) => (typeof map[key] === "string" ? (map[key] as string) : "");
  const number = (group: "background" | "render", key: string, label: string, step: number) => (
    <NumberInput
      label={label}
      value={num(group === "background" ? background : render, key)}
      step={step}
      onSet={(value) => onSetSetting(group, key, value, null)}
      onDrag={(offset, startValue) => onSetSetting(group, key, startValue + offset, `drag:${group}:${key}`)}
      onDragEnd={onEditDone}
    />
  );

  return (
    <aside className="properties">
      <div className="tab-strip">
        <span className="tab active">Project properties</span>
      </div>
      <div className="properties-scroll">
        <Section title="Project settings" open={!!open.project} onToggle={() => toggle("project")}>
          <Field label="Name">
            <TextInput value={project.name} onCommit={(value) => onSetInfo("name", value)} />
          </Field>
          <Field label="Author">
            <TextInput value={project.author} onCommit={(value) => onSetInfo("author", value)} />
          </Field>
          <Field label="Description">
            <TextInput multiline value={project.description} onCommit={(value) => onSetInfo("description", value)} />
          </Field>
          <Field label="Project location">{projectFolder(project.path)}</Field>
          <Field label="Render size">
            <div className="pair-input">
              <NumberInput
                label="W"
                value={project.videoWidth}
                step={0}
                onSet={(value) => onSetInfo("video_size", [value, project.videoHeight])}
                onDrag={() => undefined}
                onDragEnd={() => undefined}
              />
              <NumberInput
                label="H"
                value={project.videoHeight}
                step={0}
                onSet={(value) => onSetInfo("video_size", [project.videoWidth, value])}
                onDrag={() => undefined}
                onDragEnd={() => undefined}
              />
            </div>
          </Field>
          <Field label="Tempo (frames per second)">
            <NumberInput
              label="fps"
              value={project.tempo}
              step={0}
              onSet={(value) => onSetInfo("tempo", value)}
              onDrag={() => undefined}
              onDragEnd={() => undefined}
            />
          </Field>
          <Field label="Saved with">
            {project.createdIn || "unknown"} (format {project.format})
          </Field>
          {project.warnings.length > 0 && (
            <Field label="Warnings">
              <ul className="warnings">
                {project.warnings.map((warning, i) => (
                  <li key={i}>{warning}</li>
                ))}
              </ul>
            </Field>
          )}
        </Section>

        <Section title="Render settings" open={!!open.render} onToggle={() => toggle("render")}>
          <Field label="Preset">{project.renderSettings || "Custom"}</Field>
          <Field label="Samples">{number("render", "render_samples", "#", 0.1)}</Field>
          <Field label="Render distance">{number("render", "render_distance", "↔", 10)}</Field>
          <Toggle label="Ambient occlusion" on={bool(render, "render_ssao")} onChange={(on) => onSetSetting("render", "render_ssao", on, null)} />
          <Toggle label="Shadows" on={bool(render, "render_shadows")} onChange={(on) => onSetSetting("render", "render_shadows", on, null)} />
          <Toggle label="Indirect lighting" on={bool(render, "render_indirect")} onChange={(on) => onSetSetting("render", "render_indirect", on, null)} />
          <Toggle label="Reflections" on={bool(render, "render_reflections")} onChange={(on) => onSetSetting("render", "render_reflections", on, null)} />
          <p className="muted">The viewport does not use these yet; they are saved for high quality rendering.</p>
        </Section>

        <Section title="Library" open={!!open.library} onToggle={() => toggle("library")}>
          <Field label="Templates">{project.templates}</Field>
          <Field label="Timelines">
            {project.timelines.length} ({project.cameras} cameras)
          </Field>
        </Section>

        <Section title="Environment" open={!!open.environment} onToggle={() => toggle("environment")}>
          <div className="dials">
            <div className="dial">
              <span className="field-label">Time ({clockTime(num(background, "sky_time"))})</span>
              {number("background", "sky_time", "°", 0.5)}
            </div>
            <div className="dial">
              <span className="field-label">Rotation</span>
              {number("background", "sky_rotation", "°", 0.5)}
            </div>
          </div>
          {(
            [
              ["Clouds", "sky_clouds_show"],
              ["Ground", "ground_show"],
              ["Twilight", "twilight"],
              ["Fog", "fog_show"],
              ["Wind", "wind"],
            ] as const
          ).map(([label, key]) => (
            <Toggle key={key} label={label} on={bool(background, key)} onChange={(on) => onSetSetting("background", key, on, null)} />
          ))}
          <Field label="Biome">{text(background, "biome") || environment.biome}</Field>
          <div className="field-label">Scene colors:</div>
          <div className="swatches">
            {(
              [
                ["Sky", "sky_color"],
                ["Clouds", "sky_clouds_color"],
                ["Sunlight", "sunlight_color"],
                ["Ambient", "ambient_color"],
                ["Night", "night_color"],
              ] as const
            ).map(([label, key]) => (
              <Swatch
                key={key}
                label={label}
                color={text(background, key) || "#000000"}
                onPick={(color) => onSetSetting("background", key, color, `color:${key}`)}
              />
            ))}
          </div>
          <Field label="Texture animation speed">{number("background", "texture_animation_speed", "×", 0.005)}</Field>
        </Section>

        <Section title="Resources" open={!!open.resources} onToggle={() => toggle("resources")}>
          <Field label="Files used by the project">{project.resources}</Field>
        </Section>

        <Section title="Selected timeline" open={!!open.selection} onToggle={() => toggle("selection")}>
          {timeline && state ? (
            <>
              <Field label="Name">{timeline.name || "(unnamed)"}</Field>
              <Field label="Type">{timeline.kind}</Field>
              {timelineSettings?.skin && (
                <Field label="Skin">
                  <span className="skin-row">
                    <span className="skin-name">{timelineSettings.skin.file ?? "Default"}</span>
                    <button className="secondary" onClick={() => onChangeSkin(true)}>
                      Browse…
                    </button>
                    {timelineSettings.skin.file && (
                      <button className="secondary" onClick={() => onChangeSkin(false)} title="Use the texture of the Minecraft assets">
                        Default
                      </button>
                    )}
                  </span>
                </Field>
              )}
              <Field label="Position in the world">{formatVec(state.worldPosition)}</Field>
              <Field label="Visible in the scene">
                {state.visible ? "Yes" : "No"}
                {state.alpha !== state.alphaValue && ` (alpha with parents ${formatNumber(state.alpha)})`}
              </Field>
              {values.map((group) => {
                const numbers = group.values.every((v) => v.kind === "number");
                const vector = numbers && group.values.length === 3 && group.values.every((v) => v.label.length === 1);
                const control = (entry: ValueEntry, handle: string) => {
                  const set = (value: unknown) => onEditValues([{ name: entry.name, value }], "set", null);
                  switch (entry.kind) {
                    case "number":
                      return (
                        <NumberInput
                          label={handle}
                          value={entry.value as number}
                          step={entry.step}
                          onSet={set}
                          onDrag={(offset) => onEditValues([{ name: entry.name, value: offset }], "add", `drag:${entry.name}`)}
                          onDragEnd={onEditDone}
                        />
                      );
                    case "bool":
                      return <Toggle label={entry.label} on={entry.value === true} onChange={set} />;
                    case "color":
                      return (
                        <Swatch
                          label={entry.label}
                          color={entry.value as string}
                          onPick={(color) => onEditValues([{ name: entry.name, value: color }], "set", `color:${entry.name}`)}
                        />
                      );
                    case "choice":
                      return (
                        <select className="text-input" value={entry.value as string} onChange={(e) => set(e.target.value)}>
                          {entry.options.map((option) => (
                            <option key={option} value={option}>
                              {option}
                            </option>
                          ))}
                        </select>
                      );
                    default:
                      return <TextInput multiline value={entry.value as string} onCommit={set} />;
                  }
                };
                return (
                  <details
                    key={group.title}
                    className="value-group"
                    open={openGroups.has(group.title)}
                    onToggle={(e) => {
                      const isOpen = e.currentTarget.open;
                      setOpenGroups((current) => {
                        if (current.has(group.title) === isOpen) return current;
                        const next = new Set(current);
                        if (isOpen) next.add(group.title);
                        else next.delete(group.title);
                        return next;
                      });
                    }}
                  >
                    <summary>{group.title}</summary>
                    {vector ? (
                      <div className="vector-input">
                        {group.values.map((entry) => (
                          <div key={entry.name}>{control(entry, entry.label)}</div>
                        ))}
                      </div>
                    ) : (
                      group.values.map((entry) =>
                        entry.kind === "bool" || entry.kind === "color" ? (
                          <div key={entry.name} className="value-row">
                            {control(entry, "")}
                          </div>
                        ) : (
                          <div key={entry.name} className="value-row labelled">
                            <span className="field-label">{entry.label}</span>
                            {control(entry, "\u2194")}
                          </div>
                        ),
                      )
                    )}
                  </details>
                );
              })}
              {timelineSettings &&
                TIMELINE_SWITCHES.map(([group, title, switches]) => {
                  const map = timelineSettings[group];
                  if (!map) return null;
                  return (
                    <details
                      key={group}
                      className="value-group"
                      open={openGroups.has(title)}
                      onToggle={(e) => {
                        const isOpen = e.currentTarget.open;
                        setOpenGroups((current) => {
                          if (current.has(title) === isOpen) return current;
                          const next = new Set(current);
                          if (isOpen) next.add(title);
                          else next.delete(title);
                          return next;
                        });
                      }}
                    >
                      <summary>{title}</summary>
                      {switches.map(([key, label]) => (
                        <Toggle key={key} label={label} on={map[key] === true} onChange={(on) => onSetTimelineSetting(group, key, on)} />
                      ))}
                    </details>
                  );
                })}
              <Field label="Keyframes">{timeline.keyframes.length}</Field>
            </>
          ) : (
            <p className="muted">Click a timeline to see and change its values at the current frame.</p>
          )}
        </Section>
      </div>
    </aside>
  );
}
