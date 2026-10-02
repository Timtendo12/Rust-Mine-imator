import { useState, type ReactNode } from "react";
import type { FrameState, ProjectSummary, Vec3 } from "./backend";

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

function Swatch({ label, color }: { label: string; color: string }) {
  return (
    <div className="swatch">
      <div className="field-label">{label}</div>
      <div className="swatch-color" style={{ background: color }} title={color} />
    </div>
  );
}

function Toggle({ label, on }: { label: string; on: boolean }) {
  return (
    <div className="toggle-row">
      <span>{label}</span>
      <span className={on ? "switch on" : "switch"} aria-label={on ? "on" : "off"} />
    </div>
  );
}

interface Props {
  project: ProjectSummary;
  frame: FrameState | null;
  selected: string | null;
}

/**
 * The properties panel on the right. Values are shown as the project has
 * them; editing them is not available yet.
 */
export function Properties({ project, frame, selected }: Props) {
  const [open, setOpen] = useState<Record<string, boolean>>({ project: true, selection: true });
  const toggle = (key: string) => setOpen((current) => ({ ...current, [key]: !current[key] }));

  const index = selected ? project.timelines.findIndex((t) => t.id === selected) : -1;
  const timeline = index >= 0 ? project.timelines[index] : null;
  const state = frame && index >= 0 ? frame.timelines[index] : null;
  const environment = project.environment;

  return (
    <aside className="properties">
      <div className="tab-strip">
        <span className="tab active">Project properties</span>
      </div>
      <div className="properties-scroll">
        <Section title="Project settings" open={!!open.project} onToggle={() => toggle("project")}>
          <Field label="Name">{project.name || "(unnamed)"}</Field>
          <Field label="Author">{project.author || "—"}</Field>
          <Field label="Description">
            <span className="multiline">{project.description || "—"}</span>
          </Field>
          <Field label="Project location">{projectFolder(project.path)}</Field>
          <Field label="Render size">
            {project.videoWidth} × {project.videoHeight}
          </Field>
          <Field label="Tempo">{project.tempo} frames per second</Field>
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
          <Field label="Samples">{project.renderSamples}</Field>
          <p className="muted">High quality rendering is not available yet.</p>
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
              <span className="field-label">Time</span>
              <span>{clockTime(environment.skyTime)}</span>
            </div>
            <div className="dial">
              <span className="field-label">Rotation</span>
              <span>{formatNumber(environment.skyRotation)}°</span>
            </div>
          </div>
          <Toggle label="Clouds" on={environment.cloudsShow} />
          <Toggle label="Ground" on={environment.groundShow} />
          <Field label="Biome">{environment.biome}</Field>
          <div className="field-label">Scene colors:</div>
          <div className="swatches">
            <Swatch label="Sky" color={environment.skyColor} />
            <Swatch label="Clouds" color={environment.cloudsColor} />
            <Swatch label="Sunlight" color={environment.sunlightColor} />
            <Swatch label="Ambient" color={environment.ambientColor} />
            <Swatch label="Night" color={environment.nightColor} />
          </div>
          <Toggle label="Twilight" on={environment.twilight} />
          <Toggle label="Fog" on={environment.fogShow} />
          <Toggle label="Wind" on={environment.wind} />
          <Field label="Texture animation speed">{formatNumber(environment.textureAnimationSpeed)}</Field>
        </Section>

        <Section title="Resources" open={!!open.resources} onToggle={() => toggle("resources")}>
          <Field label="Files used by the project">{project.resources}</Field>
        </Section>

        <Section title="Selected timeline" open={!!open.selection} onToggle={() => toggle("selection")}>
          {timeline && state ? (
            <>
              <Field label="Name">{timeline.name || "(unnamed)"}</Field>
              <Field label="Type">{timeline.kind}</Field>
              <Field label="Position">{formatVec(state.position)}</Field>
              <Field label="Rotation">{formatVec(state.rotation)}</Field>
              <Field label="Scale">{formatVec(state.scale)}</Field>
              <Field label="Position in the world">{formatVec(state.worldPosition)}</Field>
              <Field label="Visible">{state.visible ? "Yes" : "No"}</Field>
              <Field label="Alpha">{formatNumber(state.alpha)}</Field>
              <Field label="Transition">{state.transition}</Field>
              <Field label="Keyframes">{timeline.keyframes.length}</Field>
            </>
          ) : (
            <p className="muted">Click a timeline to see its values at the current frame.</p>
          )}
        </Section>
      </div>
    </aside>
  );
}
