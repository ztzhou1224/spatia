import { useRef, useEffect, useState, lazy, Suspense } from "react";
import type { MapViewHandle } from "./components/MapView";
import { FileList } from "./components/FileList";
import { ChatCard } from "./components/ChatCard";
import { WidgetPanel } from "./components/WidgetPanel";
import { SettingsPanel } from "./components/SettingsPanel";
import { RiskWorkflow } from "./components/RiskWorkflow";
import { useAppStore } from "./lib/appStore";
import "./App.css";

const MapView = lazy(() =>
  import("./components/MapView").then((m) => ({ default: m.MapView }))
);

function App() {
  const mapViewRef = useRef<MapViewHandle>(null);
  const fetchApiConfig = useAppStore((s) => s.fetchApiConfig);
  const fetchLogPath = useAppStore((s) => s.fetchLogPath);
  const fetchDomainConfig = useAppStore((s) => s.fetchDomainConfig);
  const settingsOpen = useAppStore((s) => s.settingsOpen);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const workflowOpen = useAppStore((s) => s.workflowOpen);
  const setWorkflowOpen = useAppStore((s) => s.setWorkflowOpen);
  const [fileListCollapsed, setFileListCollapsed] = useState(false);

  useEffect(() => {
    void fetchApiConfig();
    void fetchLogPath();
    void fetchDomainConfig();
  }, [fetchApiConfig, fetchLogPath, fetchDomainConfig]);

  const panelWidth = fileListCollapsed ? 44 : 300;

  return (
    <div className="app-layout">
      <Suspense fallback={<div className="map-fill" />}>
        <MapView ref={mapViewRef} />
      </Suspense>
      <div
        className="absolute top-0 right-0 bottom-0 z-10 glass-panel border-l border-border overflow-hidden flex flex-col"
        style={{
          width: `${panelWidth}px`,
          transition: "width 250ms ease",
          boxShadow: "-4px 0 24px rgba(0,0,0,0.4), -1px 0 0 rgba(148,163,184,0.08)",
        }}
      >
        <FileList
          collapsed={fileListCollapsed}
          onToggleCollapse={() => setFileListCollapsed((c) => !c)}
          onSettingsClick={() => setSettingsOpen(true)}
        />
      </div>
      {/* Risk Workflow launch button */}
      <button
        onClick={() => setWorkflowOpen(true)}
        title="Open Risk Assessment Workflow"
        style={{
          position: "absolute",
          top: 50,
          left: 10,
          zIndex: 6,
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "7px 14px",
          fontSize: 12,
          fontWeight: 600,
          background: "rgba(15, 15, 22, 0.88)",
          border: "1px solid rgba(148, 163, 184, 0.18)",
          borderRadius: 8,
          color: "rgba(255,255,255,0.85)",
          cursor: "pointer",
          backdropFilter: "blur(12px)",
          boxShadow: "0 4px 16px rgba(0,0,0,0.4)",
        }}
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
          <path d="M7 1.5C4.5 1.5 2.5 3.5 2.5 6c0 1.5.7 2.8 1.8 3.6L4 12l2.5-1H7c2.5 0 4.5-2 4.5-4.5S9.5 1.5 7 1.5z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
          <path d="M5 5.5h4M5 7.5h2.5" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round" />
        </svg>
        Risk Assessment
      </button>
      <WidgetPanel />
      <ChatCard mapViewRef={mapViewRef} panelWidth={panelWidth} />
      <SettingsPanel open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      <RiskWorkflow open={workflowOpen} onClose={() => setWorkflowOpen(false)} />
    </div>
  );
}

export default App;
