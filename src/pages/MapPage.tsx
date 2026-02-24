import { useEffect, useMemo, useRef, useState } from "react";
import { MapboxOverlay } from "@deck.gl/mapbox";
import { ScatterplotLayer } from "@deck.gl/layers";
import maplibregl from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Protocol } from "pmtiles";
import { Box, Text, Flex, Checkbox, TextField } from "@radix-ui/themes";
import { SearchWidget } from "../components/SearchWidget";
import { AnalysisChat } from "../components/AnalysisChat";
import { isTauri } from "../lib/tauri";
import { useFocusGuard } from "../lib/useFocusGuard";
import { useWidgetStore } from "../lib/widgetStore";

const OVERTURE_RELEASE = "2026-02-18.0";
const OVERTURE_SOURCE = "Overture Maps Foundation";
const DEFAULT_DB_PATH = "./spatia.duckdb";
const DEFAULT_SEARCH_TABLE = "places_wa";

const PMTILES_SOURCE_PATHS = {
  places: "./out/places.pmtiles",
  names: "./out/names.pmtiles",
  roads: "./out/roads.pmtiles",
  buildings: "./out/buildings.pmtiles",
  boundaries: "./out/boundaries.pmtiles",
};

const protocol = new Protocol();
let protocolRegistered = false;

type LayerId = "places" | "names" | "roads" | "buildings" | "boundaries";

type LayerDefinition = {
  id: LayerId;
  label: string;
  layerType: "circle" | "symbol" | "line" | "fill";
};

const LAYER_DEFINITIONS: LayerDefinition[] = [
  { id: "places", label: "places", layerType: "circle" },
  { id: "names", label: "names", layerType: "symbol" },
  { id: "roads", label: "roads", layerType: "line" },
  { id: "buildings", label: "buildings", layerType: "fill" },
  { id: "boundaries", label: "boundaries", layerType: "line" },
];

function toSourceUrl(path: string): string {
  if (!path.trim()) {
    return "";
  }
  if (path.startsWith("http://") || path.startsWith("https://")) {
    return path;
  }
  if (isTauri() && path.startsWith("/")) {
    return convertFileSrc(path);
  }
  return path;
}

function buildStyle(
  paths: Record<LayerId, string>,
): maplibregl.StyleSpecification {
  const sources: Record<string, maplibregl.VectorSourceSpecification> = {};
  const layers: maplibregl.LayerSpecification[] = [];

  for (const layer of LAYER_DEFINITIONS) {
    const sourceUrl = toSourceUrl(paths[layer.id]);
    if (!sourceUrl) {
      continue;
    }

    sources[layer.id] = {
      type: "vector",
      url: `pmtiles://${sourceUrl}`,
      attribution: `${OVERTURE_SOURCE} (${OVERTURE_RELEASE})`,
    };

    layers.push({
      id: layer.id,
      type: layer.layerType,
      source: layer.id,
      "source-layer": layer.id,
      layout: {
        visibility: "visible",
      },
    });
  }

  return {
    version: 8,
    sources,
    layers,
  };
}

const EMPTY_STYLE: maplibregl.StyleSpecification = {
  version: 8,
  sources: {},
  layers: [],
};

const ANALYSIS_SOURCE_ID = "analysis-result-source";
const ANALYSIS_LAYER_ID = "analysis-result-layer";

export function MapPage() {
  const appFocusedWidgetId = useWidgetStore(
    (state) => state.appFocusedWidgetId,
  );
  const updateMetadata = useWidgetStore((state) => state.updateMetadata);
  const mapFocusGuard = useFocusGuard({
    id: "map-widget",
    label: "Map",
    kind: "map",
  });
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const deckOverlayRef = useRef<MapboxOverlay | null>(null);
  const [dbPath, setDbPath] = useState(DEFAULT_DB_PATH);
  const [searchTable, setSearchTable] = useState(DEFAULT_SEARCH_TABLE);
  const [analysisGeoJson, setAnalysisGeoJson] = useState<unknown>({
    type: "FeatureCollection",
    features: [],
  });
  const [visualizationCommand, setVisualizationCommand] = useState("scatter");
  const [visibleLayers, setVisibleLayers] = useState<Record<LayerId, boolean>>({
    places: true,
    names: true,
    roads: true,
    buildings: true,
    boundaries: true,
  });

  const style = useMemo(() => buildStyle(PMTILES_SOURCE_PATHS), []);

  useEffect(() => {
    if (mapRef.current || !containerRef.current) return;

    if (!protocolRegistered) {
      maplibregl.addProtocol("pmtiles", protocol.tile);
      protocolRegistered = true;
    }

    mapRef.current = new maplibregl.Map({
      container: containerRef.current,
      style: style.layers.length > 0 ? style : EMPTY_STYLE,
      center: [-122.4194, 37.7749], // San Francisco – placeholder location
      zoom: 11,
    });

    mapRef.current.addControl(new maplibregl.NavigationControl(), "top-right");
    mapRef.current.addControl(
      new maplibregl.ScaleControl({ unit: "metric" }),
      "bottom-left",
    );

    deckOverlayRef.current = new MapboxOverlay({
      interleaved: false,
      layers: [],
    });
    mapRef.current.addControl(deckOverlayRef.current);

    const syncCameraMetadata = () => {
      const map = mapRef.current;
      if (!map) {
        return;
      }
      const center = map.getCenter();
      updateMetadata("map-widget", {
        center: [center.lng, center.lat],
        zoom: map.getZoom(),
        bearing: map.getBearing(),
        pitch: map.getPitch(),
      });
    };

    syncCameraMetadata();
    mapRef.current.on("move", syncCameraMetadata);
    mapRef.current.on("zoom", syncCameraMetadata);
    mapRef.current.on("rotate", syncCameraMetadata);

    let popupCount = 0;
    const mapAny = mapRef.current as any;
    mapAny.on("popupopen", () => {
      popupCount += 1;
      updateMetadata("map-widget", { activePopups: popupCount });
    });
    mapAny.on("popupclose", () => {
      popupCount = Math.max(0, popupCount - 1);
      updateMetadata("map-widget", { activePopups: popupCount });
    });

    mapRef.current.on("click", (event) => {
      const map = mapRef.current;
      if (!map) {
        return;
      }

      const features = map.queryRenderedFeatures(event.point).slice(0, 10);
      const selectedFeatures = features
        .map((feature) => {
          if (feature.id !== undefined && feature.id !== null) {
            return String(feature.id);
          }
          const value = (
            feature.properties as Record<string, unknown> | undefined
          )?.id;
          return value ? String(value) : null;
        })
        .filter((value): value is string => Boolean(value));

      updateMetadata("map-widget", { selectedFeatures });
    });

    return () => {
      if (deckOverlayRef.current) {
        mapRef.current?.removeControl(deckOverlayRef.current);
        deckOverlayRef.current = null;
      }
      mapRef.current?.off("move", syncCameraMetadata);
      mapRef.current?.off("zoom", syncCameraMetadata);
      mapRef.current?.off("rotate", syncCameraMetadata);
      mapRef.current?.remove();
      mapRef.current = null;
    };
  }, [style, updateMetadata]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map) {
      return;
    }

    for (const layer of LAYER_DEFINITIONS) {
      if (!map.getLayer(layer.id)) {
        continue;
      }

      map.setLayoutProperty(
        layer.id,
        "visibility",
        visibleLayers[layer.id] ? "visible" : "none",
      );
    }

    const visibleLayerIds = Object.entries(visibleLayers)
      .filter(([, visible]) => visible)
      .map(([layerId]) => layerId);
    updateMetadata("map-widget", { visibleLayers: visibleLayerIds });
  }, [updateMetadata, visibleLayers]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map) {
      return;
    }

    const applyAnalysisLayer = () => {
      if (!map.getSource(ANALYSIS_SOURCE_ID)) {
        map.addSource(ANALYSIS_SOURCE_ID, {
          type: "geojson",
          data: analysisGeoJson as any,
        });
      } else {
        const source = map.getSource(
          ANALYSIS_SOURCE_ID,
        ) as maplibregl.GeoJSONSource;
        source.setData(analysisGeoJson as any);
      }

      if (!map.getLayer(ANALYSIS_LAYER_ID)) {
        map.addLayer({
          id: ANALYSIS_LAYER_ID,
          type: "circle",
          source: ANALYSIS_SOURCE_ID,
        });
      }
    };

    if (map.isStyleLoaded()) {
      applyAnalysisLayer();
    } else {
      map.once("load", applyAnalysisLayer);
    }
  }, [analysisGeoJson]);

  useEffect(() => {
    if (!deckOverlayRef.current) {
      return;
    }

    const featureCollection = analysisGeoJson as {
      type?: string;
      features?: Array<{
        geometry?: { type?: string; coordinates?: number[] };
      }>;
    };

    const pointData = (featureCollection.features ?? [])
      .filter((feature) => feature.geometry?.type === "Point")
      .map((feature) => {
        const coords = feature.geometry?.coordinates ?? [0, 0];
        return {
          position: [coords[0] ?? 0, coords[1] ?? 0] as [number, number],
        };
      });

    const layers =
      visualizationCommand === "scatter"
        ? [
            new ScatterplotLayer({
              id: "analysis-scatter",
              data: pointData,
              getPosition: (d: { position: [number, number] }) => d.position,
              getRadius: 40,
              pickable: true,
            }),
          ]
        : [];

    deckOverlayRef.current.setProps({ layers });
  }, [analysisGeoJson, visualizationCommand]);

  function setLayerVisibility(id: LayerId, next: boolean) {
    setVisibleLayers((prev) => ({ ...prev, [id]: next }));
  }

  return (
    <div className="map-wrapper">
      <div
        ref={containerRef}
        className={`map-container ${appFocusedWidgetId === "map-widget" ? "widget-focus-ring" : ""}`}
        {...mapFocusGuard}
      />

      <SearchWidget mapRef={mapRef} dbPath={dbPath} tableName={searchTable} />

      <Box
        style={{
          width: 300,
          flexShrink: 0,
          borderLeft: "1px solid var(--gray-a4)",
          background: "var(--color-panel-solid)",
          overflowY: "auto",
        }}
        p="3"
      >
        <Text size="2" weight="bold" as="div" mb="2">
          Layers
        </Text>

        <Text size="1" weight="medium" as="div" mb="1">
          Search DB path
        </Text>
        <TextField.Root
          value={dbPath}
          onChange={(e) => setDbPath(e.target.value)}
          mb="2"
        />

        <Text size="1" weight="medium" as="div" mb="1">
          Search table
        </Text>
        <TextField.Root
          value={searchTable}
          onChange={(e) => setSearchTable(e.target.value)}
          mb="3"
        />

        <Flex direction="column" gap="2" mb="3">
          {LAYER_DEFINITIONS.map((layer) => (
            <Flex key={layer.id} align="center" gap="2">
              <Checkbox
                checked={visibleLayers[layer.id]}
                onCheckedChange={(value) =>
                  setLayerVisibility(layer.id, value === true)
                }
              />
              <Text size="2">{layer.label}</Text>
            </Flex>
          ))}
        </Flex>

        <Box
          p="2"
          style={{
            border: "1px solid var(--gray-a4)",
            borderRadius: "var(--radius-2)",
          }}
        >
          <Text size="1" weight="bold" as="div" mb="1">
            Attribution
          </Text>
          <Text size="1" color="gray" as="div">
            Source: {OVERTURE_SOURCE}
          </Text>
          <Text size="1" color="gray" as="div">
            Release: {OVERTURE_RELEASE}
          </Text>
          <Text size="1" color="gray" as="div">
            PMTiles: {Object.values(PMTILES_SOURCE_PATHS).join(", ")}
          </Text>
        </Box>

        <AnalysisChat
          dbPath={dbPath}
          tableName={searchTable}
          onGeoJson={setAnalysisGeoJson}
          onVisualizationCommand={setVisualizationCommand}
        />
      </Box>
    </div>
  );
}
