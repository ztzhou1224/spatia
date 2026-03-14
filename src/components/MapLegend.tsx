import { useAppStore } from "../lib/appStore";

export function MapLegend() {
  const analysisGeoJson = useAppStore((s) => s.analysisGeoJson);
  const visualizationType = useAppStore((s) => s.visualizationType);

  const features = (analysisGeoJson as { features?: unknown[] })?.features ?? [];
  if (features.length === 0) return null;

  const vizType = visualizationType ?? "scatter";

  return (
    <div
      style={{
        position: "absolute",
        bottom: 70,
        left: 10,
        zIndex: 5,
        background: "rgba(15, 15, 20, 0.85)",
        backdropFilter: "blur(8px)",
        borderRadius: 8,
        padding: "8px 12px",
        minWidth: 120,
        border: "1px solid rgba(148, 163, 184, 0.15)",
        pointerEvents: "auto",
      }}
    >
      <div style={{ fontSize: 10, fontWeight: 600, color: "rgba(255,255,255,0.7)", marginBottom: 6, textTransform: "uppercase", letterSpacing: "0.5px" }}>
        {vizType === "heatmap" ? "Density" : vizType === "hexbin" ? "Aggregated Value" : "Properties"}
      </div>
      {vizType === "scatter" && (
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <div
            style={{
              width: 12,
              height: 12,
              borderRadius: "50%",
              backgroundColor: "#7c3aed",
              border: "1px solid rgba(255,255,255,0.3)",
            }}
          />
          <span style={{ fontSize: 11, color: "rgba(255,255,255,0.8)" }}>
            {features.length} point{features.length !== 1 ? "s" : ""}
          </span>
        </div>
      )}
      {(vizType === "heatmap" || vizType === "hexbin") && (
        <div>
          <div
            style={{
              height: 10,
              borderRadius: 4,
              background: "linear-gradient(to right, rgb(63,0,125), rgb(84,42,143), rgb(107,52,168), rgb(124,58,237), rgb(167,139,250), rgb(221,214,254))",
            }}
          />
          <div style={{ display: "flex", justifyContent: "space-between", marginTop: 3 }}>
            <span style={{ fontSize: 9, color: "rgba(255,255,255,0.5)" }}>Low</span>
            <span style={{ fontSize: 9, color: "rgba(255,255,255,0.5)" }}>High</span>
          </div>
        </div>
      )}
    </div>
  );
}
