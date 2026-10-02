import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { cancelExport, exportMovie, type MovieFormat, type ProjectSummary } from "./backend";

/** Video qualities of the original, as bit rates. */
const QUALITIES: [string, number][] = [
  ["Best", 5000000],
  ["High", 2500000],
  ["Medium", 1200000],
  ["Low", 700000],
  ["Very low", 350000],
];

const FORMATS: [MovieFormat, string][] = [
  ["mp4", "MP4 video (.mp4)"],
  ["mov", "QuickTime video (.mov)"],
  ["wmv", "Windows Media video (.wmv)"],
  ["png", "Image sequence (.png)"],
];

const FRAME_RATES = [24, 30, 60];

interface Props {
  project: ProjectSummary;
  onClose: () => void;
  onError: (message: string) => void;
}

/** The "Export animation" popup: format, quality and frame rate, then the frames as they are made. */
export function ExportDialog({ project, onClose, onError }: Props) {
  const [format, setFormat] = useState<MovieFormat>("mp4");
  const [bitRate, setBitRate] = useState(2500000);
  const [customQuality, setCustomQuality] = useState(false);
  const [frameRate, setFrameRate] = useState(30);
  const [customRate, setCustomRate] = useState(false);
  const [progress, setProgress] = useState<{ frame: number; total: number } | null>(null);
  const [result, setResult] = useState<string | null>(null);

  useEffect(() => {
    const stop = listen<{ frame: number; total: number }>("export-progress", (event) => setProgress(event.payload));
    return () => {
      void stop.then((off) => off());
    };
  }, []);

  const exporting = progress !== null && result === null;

  const start = async () => {
    const path = await save({
      filters: [{ name: FORMATS.find(([f]) => f === format)![1], extensions: [format] }],
      defaultPath: `${project.name || "Untitled"}.${format}`,
    });
    if (typeof path !== "string") return;
    setResult(null);
    setProgress({ frame: 0, total: 0 });
    try {
      const done = await exportMovie(path, format, frameRate, bitRate);
      setResult(done.cancelled ? `Stopped after ${done.frames} frames.` : `Exported ${done.frames} frames.`);
    } catch (e) {
      setProgress(null);
      onError(String(e));
    }
  };

  const seconds = project.tempo > 0 ? project.length / project.tempo : 0;

  return (
    <div className="modal-backdrop">
      <div className="modal" role="dialog" aria-label="Export animation">
        <h2>Export animation</h2>
        {progress === null ? (
          <>
            <label className="modal-row">
              <span>Format</span>
              <select value={format} onChange={(e) => setFormat(e.target.value as MovieFormat)}>
                {FORMATS.map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
            {format !== "png" && (
              <label className="modal-row">
                <span>Quality</span>
                <select
                  value={customQuality ? "custom" : String(bitRate)}
                  onChange={(e) => {
                    setCustomQuality(e.target.value === "custom");
                    if (e.target.value !== "custom") setBitRate(Number(e.target.value));
                  }}
                >
                  {QUALITIES.map(([label, rate]) => (
                    <option key={rate} value={rate}>
                      {label}
                    </option>
                  ))}
                  <option value="custom">Custom</option>
                </select>
              </label>
            )}
            {format !== "png" && customQuality && (
              <label className="modal-row">
                <span>Bit rate</span>
                <input type="number" min={1} step={500} value={bitRate} onChange={(e) => setBitRate(Math.max(1, Number(e.target.value) || 1))} />
              </label>
            )}
            <label className="modal-row">
              <span>Frame rate</span>
              <select
                value={customRate ? "custom" : String(frameRate)}
                onChange={(e) => {
                  setCustomRate(e.target.value === "custom");
                  if (e.target.value !== "custom") setFrameRate(Number(e.target.value));
                }}
              >
                {FRAME_RATES.map((rate) => (
                  <option key={rate} value={rate}>
                    {rate}
                  </option>
                ))}
                <option value="custom">Custom</option>
              </select>
            </label>
            {customRate && (
              <label className="modal-row">
                <span>Frames per second</span>
                <input
                  type="number"
                  min={1}
                  max={120}
                  value={frameRate}
                  onChange={(e) => setFrameRate(Math.min(120, Math.max(1, Math.round(Number(e.target.value)) || 1)))}
                />
              </label>
            )}
            <p className="modal-note">
              {project.videoWidth} × {project.videoHeight}, {seconds.toFixed(2)} seconds, {Math.floor(seconds * frameRate) + 1} frames, through the
              active camera (the work camera if there is none).
            </p>
            <div className="modal-buttons">
              <button className="secondary" onClick={onClose}>
                Cancel
              </button>
              <button onClick={() => void start()}>Save…</button>
            </div>
          </>
        ) : (
          <>
            <progress value={result ? 1 : progress.total > 0 ? progress.frame / progress.total : 0} />
            <p className="modal-note">{result ?? `Frame ${progress.frame + 1} of ${progress.total}`}</p>
            <div className="modal-buttons">
              {exporting ? (
                <button className="secondary" onClick={() => void cancelExport()}>
                  Stop
                </button>
              ) : (
                <button onClick={onClose}>Close</button>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
