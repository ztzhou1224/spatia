import { useRef, useEffect, useState, lazy, Suspense } from "react";
import type { MapViewHandle } from "./components/MapView";
import { FileList } from "./components/FileList";
import { ChatCard } from "./components/ChatCard";
import { WidgetPanel } from "./components/WidgetPanel";
import { useAppStore } from "./lib/appStore";
import "./App.css";

const MapView = lazy(() =>
  import("./components/MapView").then((m) => ({ default: m.MapView }))
);

function App() {
  const mapViewRef = useRef<MapViewHandle>(null);
  const fetchApiConfig = useAppStore((s) => s.fetchApiConfig);
  const fetchLogPath = useAppStore((s) => s.fetchLogPath);
  const [fileListCollapsed, setFileListCollapsed] = useState(false);

  useEffect(() => {
    void fetchApiConfig();
    void fetchLogPath();
  }, [fetchApiConfig, fetchLogPath]);

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
        />
      </div>
      <WidgetPanel />
      <ChatCard mapViewRef={mapViewRef} panelWidth={panelWidth} />
    </div>
  );
}

export default App;
