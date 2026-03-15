import { useAppStore } from "../lib/appStore";

const BASEMAP_OPTIONS = [
  { id: "dark", label: "Dark" },
  { id: "light", label: "Light" },
  { id: "osm", label: "OSM" },
] as const;

export function BasemapSelector({ left = 10 }: { left?: number }) {
  const basemapId = useAppStore((s) => s.basemapId);
  const setBasemapId = useAppStore((s) => s.setBasemapId);

  return (
    <div
      style={{
        position: "absolute",
        top: 10,
        left,
        zIndex: 5,
        display: "flex",
        gap: 1,
        background: "rgba(15, 15, 20, 0.85)",
        backdropFilter: "blur(8px)",
        borderRadius: 6,
        padding: 2,
        border: "1px solid rgba(148, 163, 184, 0.15)",
      }}
    >
      {BASEMAP_OPTIONS.map((opt) => (
        <button
          key={opt.id}
          onClick={() => setBasemapId(opt.id)}
          title={`Switch to ${opt.label} basemap`}
          style={{
            padding: "4px 10px",
            fontSize: 11,
            fontWeight: basemapId === opt.id ? 600 : 400,
            color: basemapId === opt.id ? "#fff" : "rgba(255,255,255,0.55)",
            background: basemapId === opt.id ? "rgba(124, 58, 237, 0.5)" : "transparent",
            border: "none",
            borderRadius: 4,
            cursor: "pointer",
            transition: "all 150ms ease",
          }}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}
