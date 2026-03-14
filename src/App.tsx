import { useRef, useEffect, useState, lazy, Suspense } from "react";
import type { MapViewHandle } from "./components/MapView";
import { FileList } from "./components/FileList";
import { ChatCard } from "./components/ChatCard";
import { WidgetPanel } from "./components/WidgetPanel";
import { SettingsPanel } from "./components/SettingsPanel";
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
      <WidgetPanel />
      <ChatCard mapViewRef={mapViewRef} panelWidth={panelWidth} />
      <SettingsPanel open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}

export default App;
