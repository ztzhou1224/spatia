import { useEffect, useRef, useImperativeHandle, forwardRef, useCallback, useState } from "react";
import { MapboxOverlay } from "@deck.gl/mapbox";
import { ScatterplotLayer } from "@deck.gl/layers";
import { HeatmapLayer, HexagonLayer } from "@deck.gl/aggregation-layers";
import maplibregl from "maplibre-gl";
import { save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
// maplibre-gl.css is imported in App.css (after Tailwind) to ensure correct cascade priority
import { useAppStore } from "../lib/appStore";
import { isTauri } from "../lib/tauri";
import { MapLegend } from "./MapLegend";
import { BasemapSelector } from "./BasemapSelector";
import { LayerPanel } from "./LayerPanel";

/** Basemap style definitions */
const BASEMAP_STYLES: Record<string, maplibregl.StyleSpecification> = {
  dark: {
    version: 8,
    sources: {
      basemap: {
        type: "raster",
        tiles: [
          "https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
          "https://b.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
          "https://c.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
        ],
        tileSize: 512,
        attribution:
          '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> &copy; <a href="https://carto.com/">CARTO</a>',
        maxzoom: 20,
      },
    },
    layers: [{ id: "basemap", type: "raster", source: "basemap" }],
  },
  light: {
    version: 8,
    sources: {
      basemap: {
        type: "raster",
        tiles: [
          "https://a.basemaps.cartocdn.com/light_all/{z}/{x}/{y}@2x.png",
          "https://b.basemaps.cartocdn.com/light_all/{z}/{x}/{y}@2x.png",
          "https://c.basemaps.cartocdn.com/light_all/{z}/{x}/{y}@2x.png",
        ],
        tileSize: 512,
        attribution:
          '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> &copy; <a href="https://carto.com/">CARTO</a>',
        maxzoom: 20,
      },
    },
    layers: [{ id: "basemap", type: "raster", source: "basemap" }],
  },
  osm: {
    version: 8,
    sources: {
      basemap: {
        type: "raster",
        tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
        tileSize: 256,
        attribution:
          '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
        maxzoom: 19,
      },
    },
    layers: [{ id: "basemap", type: "raster", source: "basemap" }],
  },
};

function getBasemapStyle(id: string): maplibregl.StyleSpecification {
  return BASEMAP_STYLES[id] ?? BASEMAP_STYLES.dark;
}

const ANALYSIS_SOURCE_ID = "analysis-result-source";
const ANALYSIS_LAYER_ID = "analysis-result-circles";
const ANALYSIS_FILL_LAYER_ID = "analysis-result-fill";
const ANALYSIS_LINE_LAYER_ID = "analysis-result-line";

const BUILDINGS_SOURCE_ID = "buildings-3d";
const BUILDINGS_LAYER_ID = "buildings-3d-layer";
const BUILDINGS_MIN_ZOOM = 14;

// Prefix for MapLibre table geometry layers (polygon/line data from spatial files)
const TABLE_SOURCE_PREFIX = "table-geom-";
const TABLE_FILL_PREFIX = "table-fill-";
const TABLE_LINE_PREFIX = "table-line-";
const TABLE_CIRCLE_PREFIX = "table-circle-";

// Prefix for risk overlay layers
const RISK_SOURCE_PREFIX = "risk-geom-";
const RISK_FILL_PREFIX = "risk-fill-";
const RISK_LINE_PREFIX = "risk-line-";
const RISK_CIRCLE_PREFIX = "risk-circle-";

// Blue color for table data points (distinct from purple analysis results)
const TABLE_POINT_COLOR: [number, number, number, number] = [37, 99, 235, 200];

export type MapViewHandle = {
  getMap: () => maplibregl.Map | null;
};

export const MapView = forwardRef<MapViewHandle>(function MapView(_props, ref) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const deckOverlayRef = useRef<MapboxOverlay | null>(null);
  const analysisGeoJson = useAppStore((s) => s.analysisGeoJson);
  const visualizationType = useAppStore((s) => s.visualizationType);
  const tableGeoJson = useAppStore((s) => s.tableGeoJson);
  const tables = useAppStore((s) => s.tables);
  const basemapId = useAppStore((s) => s.basemapId);
  const analysisTotalCount = useAppStore((s) => s.analysisTotalCount);
  const layerVisibility = useAppStore((s) => s.layerVisibility);
  const layerOpacity = useAppStore((s) => s.layerOpacity);
  const riskLayerGeoJson = useAppStore((s) => s.riskLayerGeoJson);
  const [exporting, setExporting] = useState(false);
  const buildingsFetchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastBuildingsBboxRef = useRef<string | null>(null);

  // Show welcome overlay when there are no tables and no analysis results
  const analysisFeatures = (analysisGeoJson as { features?: unknown[] })?.features ?? [];
  const hasData = tables.length > 0 || Object.keys(tableGeoJson).length > 0 || analysisFeatures.length > 0;

  useImperativeHandle(ref, () => ({
    getMap: () => mapRef.current,
  }));

  // Apply analysis MapLibre layers (circle, fill, line) to the map
  const applyAnalysisLayer = useCallback(() => {
    const map = mapRef.current;
    if (!map) return;
    const fc = useAppStore.getState().analysisGeoJson as any;
    const config = useAppStore.getState().domainConfig;
    const visibility = useAppStore.getState().layerVisibility;
    const opacity = useAppStore.getState().layerOpacity;
    const analysisVisible = visibility["analysis"] !== false;
    const analysisOpacity = opacity["analysis"] ?? 1;
    const mapVisibility = analysisVisible ? "visible" : "none";

    // Remove old layers
    for (const layerId of [ANALYSIS_LAYER_ID, ANALYSIS_FILL_LAYER_ID, ANALYSIS_LINE_LAYER_ID]) {
      if (map.getLayer(layerId)) map.removeLayer(layerId);
    }

    if (!map.getSource(ANALYSIS_SOURCE_ID)) {
      map.addSource(ANALYSIS_SOURCE_ID, { type: "geojson", data: fc });
    } else {
      (map.getSource(ANALYSIS_SOURCE_ID) as maplibregl.GeoJSONSource).setData(fc);
    }

    // Auto-fit map to show all features
    const features = fc?.features as Array<{ geometry?: { type?: string; coordinates?: any } }> | undefined;
    if (features && features.length > 0) {
      const bounds = new maplibregl.LngLatBounds();
      let hasValidCoords = false;
      for (const f of features) {
        if (!f.geometry?.coordinates) continue;
        const type = f.geometry.type;
        if (type === "Point") {
          bounds.extend(f.geometry.coordinates as [number, number]);
          hasValidCoords = true;
        } else if (type === "LineString" || type === "MultiPoint") {
          for (const coord of f.geometry.coordinates) {
            bounds.extend(coord as [number, number]);
            hasValidCoords = true;
          }
        } else if (type === "Polygon" || type === "MultiLineString") {
          for (const ring of f.geometry.coordinates) {
            for (const coord of ring) {
              bounds.extend(coord as [number, number]);
              hasValidCoords = true;
            }
          }
        } else if (type === "MultiPolygon") {
          for (const poly of f.geometry.coordinates) {
            for (const ring of poly) {
              for (const coord of ring) {
                bounds.extend(coord as [number, number]);
                hasValidCoords = true;
              }
            }
          }
        }
      }
      if (hasValidCoords && !bounds.isEmpty()) {
        map.fitBounds(bounds, { padding: 60, maxZoom: 15, duration: 1000 });
      }
    }

    // Add circle layer for points
    map.addLayer({
      id: ANALYSIS_LAYER_ID,
      type: "circle",
      source: ANALYSIS_SOURCE_ID,
      filter: ["==", ["geometry-type"], "Point"],
      layout: { visibility: mapVisibility },
      paint: {
        "circle-radius": 6,
        "circle-color": config.ui_config.primary_color,
        "circle-stroke-width": 1,
        "circle-stroke-color": "#fff",
        "circle-opacity": 0.8 * analysisOpacity,
      },
    });

    // Add fill layer for polygons
    map.addLayer({
      id: ANALYSIS_FILL_LAYER_ID,
      type: "fill",
      source: ANALYSIS_SOURCE_ID,
      filter: ["in", ["geometry-type"], ["literal", ["Polygon", "MultiPolygon"]]],
      layout: { visibility: mapVisibility },
      paint: {
        "fill-color": config.ui_config.primary_color,
        "fill-opacity": 0.3 * analysisOpacity,
      },
    });

    // Add line layer for lines
    map.addLayer({
      id: ANALYSIS_LINE_LAYER_ID,
      type: "line",
      source: ANALYSIS_SOURCE_ID,
      filter: ["in", ["geometry-type"], ["literal", ["LineString", "MultiLineString"]]],
      layout: { visibility: mapVisibility },
      paint: {
        "line-color": config.ui_config.primary_color,
        "line-width": 2,
        "line-opacity": analysisOpacity,
      },
    });

    // Click handler for feature popups
    map.on("click", ANALYSIS_LAYER_ID, (e) => {
      if (!e.features?.length) return;
      const props = e.features[0].properties;
      const html = Object.entries(props || {})
        .map(([k, v]) => `<strong>${k}:</strong> ${v}`)
        .join("<br>");
      new maplibregl.Popup()
        .setLngLat(e.lngLat)
        .setHTML(html)
        .addTo(map);
    });

    map.on("mouseenter", ANALYSIS_LAYER_ID, () => {
      map.getCanvas().style.cursor = "pointer";
    });
    map.on("mouseleave", ANALYSIS_LAYER_ID, () => {
      map.getCanvas().style.cursor = "";
    });
  }, []);

  // Add buildings GeoJSON source and fill-extrusion layer (idempotent)
  const setupBuildingsLayer = useCallback(() => {
    const map = mapRef.current;
    if (!map) return;

    if (!map.getSource(BUILDINGS_SOURCE_ID)) {
      map.addSource(BUILDINGS_SOURCE_ID, {
        type: "geojson",
        data: { type: "FeatureCollection", features: [] },
      });
    }

    if (!map.getLayer(BUILDINGS_LAYER_ID)) {
      map.addLayer({
        id: BUILDINGS_LAYER_ID,
        type: "fill-extrusion",
        source: BUILDINGS_SOURCE_ID,
        minzoom: BUILDINGS_MIN_ZOOM,
        paint: {
          "fill-extrusion-color": "#ddd",
          "fill-extrusion-height": [
            "coalesce",
            ["*", ["get", "num_floors"], 3.5],
            ["get", "height"],
            10,
          ],
          "fill-extrusion-base": 0,
          "fill-extrusion-opacity": 0.6,
        },
      });
    }
  }, []);

  // Fetch buildings for the current viewport and update the source
  const fetchBuildings = useCallback(async (map: maplibregl.Map) => {
    if (!isTauri()) return;

    const zoom = map.getZoom();
    if (zoom < BUILDINGS_MIN_ZOOM) {
      // Clear buildings data when zoomed out
      const src = map.getSource(BUILDINGS_SOURCE_ID) as maplibregl.GeoJSONSource | undefined;
      if (src) {
        src.setData({ type: "FeatureCollection", features: [] });
        lastBuildingsBboxRef.current = null;
      }
      return;
    }

    const bounds = map.getBounds();
    const bboxStr = `${bounds.getWest()},${bounds.getSouth()},${bounds.getEast()},${bounds.getNorth()}`;

    // Skip fetch if viewport hasn't changed significantly (same bbox string)
    if (bboxStr === lastBuildingsBboxRef.current) return;
    lastBuildingsBboxRef.current = bboxStr;

    try {
      const raw = await invoke<string>("fetch_buildings_in_view", { bboxStr });
      const geojson = JSON.parse(raw) as GeoJSON.FeatureCollection;
      const src = map.getSource(BUILDINGS_SOURCE_ID) as maplibregl.GeoJSONSource | undefined;
      if (src) {
        src.setData(geojson);
      }
    } catch (e) {
      console.warn("Failed to fetch buildings:", e);
    }
  }, []);

  // Debounced idle handler — schedules fetchBuildings 400 ms after the last idle event
  const debouncedFetchBuildings = useCallback(
    (map: maplibregl.Map) => {
      if (buildingsFetchTimerRef.current !== null) {
        clearTimeout(buildingsFetchTimerRef.current);
      }
      buildingsFetchTimerRef.current = setTimeout(() => {
        buildingsFetchTimerRef.current = null;
        void fetchBuildings(map);
      }, 400);
    },
    [fetchBuildings]
  );

  // Initialize map once
  useEffect(() => {
    if (mapRef.current || !containerRef.current) return;

    const { map_default_center, map_default_zoom } =
      useAppStore.getState().domainConfig.ui_config;
    const initialBasemap = useAppStore.getState().basemapId;
    mapRef.current = new maplibregl.Map({
      container: containerRef.current,
      style: getBasemapStyle(initialBasemap),
      center: map_default_center as [number, number],
      zoom: map_default_zoom,
      ...(({ preserveDrawingBuffer: true }) as any),
    } as maplibregl.MapOptions);

    mapRef.current.addControl(new maplibregl.NavigationControl(), "top-right");
    mapRef.current.addControl(
      new maplibregl.ScaleControl({ unit: "metric" }),
      "bottom-left"
    );

    deckOverlayRef.current = new MapboxOverlay({
      interleaved: false,
      layers: [],
    });
    mapRef.current.addControl(deckOverlayRef.current);

    // Set up buildings layer and idle-based fetching after style loads
    const map = mapRef.current;
    map.on("load", () => {
      setupBuildingsLayer();
    });

    const idleHandler = () => debouncedFetchBuildings(map);
    map.on("idle", idleHandler);

    return () => {
      if (buildingsFetchTimerRef.current !== null) {
        clearTimeout(buildingsFetchTimerRef.current);
        buildingsFetchTimerRef.current = null;
      }
      map.off("idle", idleHandler);
      if (deckOverlayRef.current) {
        mapRef.current?.removeControl(deckOverlayRef.current);
        deckOverlayRef.current = null;
      }
      mapRef.current?.remove();
      mapRef.current = null;
    };
  }, []);

  // Handle basemap changes
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;

    const newStyle = getBasemapStyle(basemapId);
    // Check if this is the initial style (already set in constructor)
    const currentStyle = map.getStyle();
    const currentTiles = (currentStyle?.sources?.basemap as any)?.tiles?.[0];
    const newTiles = (newStyle.sources?.basemap as any)?.tiles?.[0];
    if (currentTiles === newTiles) return;

    map.setStyle(newStyle);
    map.once("style.load", () => {
      // Re-apply analysis layers and buildings layer after style change
      setupBuildingsLayer();
      // Reset bbox cache so buildings are re-fetched with the new style
      lastBuildingsBboxRef.current = null;
      applyAnalysisLayer();
    });
  }, [basemapId, applyAnalysisLayer, setupBuildingsLayer]);

  // Update analysis GeoJSON layers
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;

    if (map.isStyleLoaded()) {
      applyAnalysisLayer();
    } else {
      map.once("load", applyAnalysisLayer);
    }
  }, [analysisGeoJson, layerVisibility, layerOpacity, applyAnalysisLayer]);

  // Deck.gl overlay — table data (blue) rendered below analysis results (purple/heatmap/hexbin)
  useEffect(() => {
    if (!deckOverlayRef.current) return;

    const layers = [];

    // Table data layers (blue) — one ScatterplotLayer per table, rendered first (bottom)
    for (const [tableName, rawFc] of Object.entries(tableGeoJson)) {
      const tableLayerId = `table-${tableName}`;
      const tableVisible = layerVisibility[tableLayerId] !== false;
      if (!tableVisible) continue;

      const tableOpacity = layerOpacity[tableLayerId] ?? 1;
      const fc = rawFc as {
        type?: string;
        features?: Array<{ geometry?: { type?: string; coordinates?: number[] } }>;
      };
      const pointData = (fc.features ?? [])
        .filter((f) => f.geometry?.type === "Point")
        .map((f) => ({
          position: [
            f.geometry?.coordinates?.[0] ?? 0,
            f.geometry?.coordinates?.[1] ?? 0,
          ] as [number, number],
        }));

      if (pointData.length > 0) {
        const baseColor = TABLE_POINT_COLOR;
        const fillColor: [number, number, number, number] = [
          baseColor[0],
          baseColor[1],
          baseColor[2],
          Math.round(baseColor[3] * tableOpacity),
        ];
        layers.push(
          new ScatterplotLayer({
            id: `table-scatter-${tableName}`,
            data: pointData,
            getPosition: (d: { position: [number, number] }) => d.position,
            getRadius: 40,
            getFillColor: fillColor,
            pickable: true,
          })
        );
      }
    }

    // Analysis result layer — rendered on top, type determined by visualizationType
    const analysisVisible = layerVisibility["analysis"] !== false;
    const analysisOpacity = layerOpacity["analysis"] ?? 1;

    if (analysisVisible) {
      const analysisFc = analysisGeoJson as {
        type?: string;
        features?: Array<{
          geometry?: { type?: string; coordinates?: number[] };
          properties?: Record<string, unknown>;
        }>;
      };
      const analysisPointData = (analysisFc.features ?? [])
        .filter((f) => f.geometry?.type === "Point")
        .map((f) => ({
          position: [
            f.geometry?.coordinates?.[0] ?? 0,
            f.geometry?.coordinates?.[1] ?? 0,
          ] as [number, number],
          properties: f.properties ?? {},
        }));

      if (analysisPointData.length > 0) {
        const vizType = visualizationType ?? "scatter";

        if (vizType === "heatmap") {
          layers.push(
            new HeatmapLayer({
              id: "analysis-heatmap",
              data: analysisPointData,
              getPosition: (d: { position: [number, number] }) => d.position,
              getWeight: (d: { properties: Record<string, unknown> }) => {
                // Use first numeric property as weight if available, else 1
                const vals = Object.values(d.properties).filter(
                  (v) => typeof v === "number" && isFinite(v as number)
                ) as number[];
                return vals.length > 0 ? Math.abs(vals[0]) : 1;
              },
              radiusPixels: 40,
              colorRange: [
                [63, 0, 125, 0],
                [84, 42, 143, Math.round(80 * analysisOpacity)],
                [107, 52, 168, Math.round(150 * analysisOpacity)],
                [124, 58, 237, Math.round(200 * analysisOpacity)],
                [167, 139, 250, Math.round(220 * analysisOpacity)],
                [221, 214, 254, Math.round(255 * analysisOpacity)],
              ],
            })
          );
        } else if (vizType === "hexbin") {
          layers.push(
            new HexagonLayer({
              id: "analysis-hexbin",
              data: analysisPointData,
              getPosition: (d: { position: [number, number] }) => d.position,
              radius: 500,
              elevationScale: 0,
              extruded: false,
              colorRange: [
                [63, 0, 125, Math.round(80 * analysisOpacity)],
                [84, 42, 143, Math.round(130 * analysisOpacity)],
                [107, 52, 168, Math.round(170 * analysisOpacity)],
                [124, 58, 237, Math.round(200 * analysisOpacity)],
                [167, 139, 250, Math.round(220 * analysisOpacity)],
                [221, 214, 254, Math.round(255 * analysisOpacity)],
              ],
              pickable: true,
            })
          );
        } else {
          // Default: scatter
          layers.push(
            new ScatterplotLayer({
              id: "analysis-scatter",
              data: analysisPointData,
              getPosition: (d: { position: [number, number] }) => d.position,
              getRadius: 40,
              getFillColor: [124, 58, 237, Math.round(180 * analysisOpacity)],
              pickable: true,
            })
          );
        }
      }
    }

    deckOverlayRef.current.setProps({ layers });
  }, [analysisGeoJson, visualizationType, tableGeoJson, layerVisibility, layerOpacity]);

  // MapLibre layers for table polygon/line geometries (spatial file imports)
  // Points are handled by Deck.gl ScatterplotLayer above; polygons and lines
  // need MapLibre fill/line layers for proper rendering.
  useEffect(() => {
    const map = mapRef.current;
    if (!map || !map.isStyleLoaded()) return;

    // Track which table sources currently exist so we can remove stale ones
    const activeSources = new Set<string>();

    for (const [tableName, rawFc] of Object.entries(tableGeoJson)) {
      const fc = rawFc as GeoJSON.FeatureCollection;
      if (!fc.features?.length) continue;

      // Check if this table has any non-point geometries
      const hasNonPointGeom = fc.features.some(
        (f) => f.geometry && f.geometry.type !== "Point"
      );
      if (!hasNonPointGeom) continue;

      const sourceId = `${TABLE_SOURCE_PREFIX}${tableName}`;
      const fillId = `${TABLE_FILL_PREFIX}${tableName}`;
      const lineId = `${TABLE_LINE_PREFIX}${tableName}`;
      const circleId = `${TABLE_CIRCLE_PREFIX}${tableName}`;
      activeSources.add(sourceId);

      if (!map.getSource(sourceId)) {
        map.addSource(sourceId, { type: "geojson", data: fc });
      } else {
        (map.getSource(sourceId) as maplibregl.GeoJSONSource).setData(fc);
      }

      const tableLayerId = `table-${tableName}`;
      const tableVisible = layerVisibility[tableLayerId] !== false;
      const tableOpacity = layerOpacity[tableLayerId] ?? 1;
      const tableMapVisibility = tableVisible ? "visible" : "none";

      // Fill layer for polygons
      if (!map.getLayer(fillId)) {
        map.addLayer({
          id: fillId,
          type: "fill",
          source: sourceId,
          filter: ["in", ["geometry-type"], ["literal", ["Polygon", "MultiPolygon"]]],
          layout: { visibility: tableMapVisibility },
          paint: {
            "fill-color": "#2563eb",
            "fill-opacity": 0.25 * tableOpacity,
          },
        });
      } else {
        map.setLayoutProperty(fillId, "visibility", tableMapVisibility);
        map.setPaintProperty(fillId, "fill-opacity", 0.25 * tableOpacity);
      }

      // Line layer for lines and polygon outlines
      if (!map.getLayer(lineId)) {
        map.addLayer({
          id: lineId,
          type: "line",
          source: sourceId,
          filter: ["in", ["geometry-type"], ["literal", ["LineString", "MultiLineString", "Polygon", "MultiPolygon"]]],
          layout: { visibility: tableMapVisibility },
          paint: {
            "line-color": "#2563eb",
            "line-width": 1.5,
            "line-opacity": 0.7 * tableOpacity,
          },
        });
      } else {
        map.setLayoutProperty(lineId, "visibility", tableMapVisibility);
        map.setPaintProperty(lineId, "line-opacity", 0.7 * tableOpacity);
      }

      // Circle layer for points in spatial files (rendered via MapLibre alongside polygons)
      if (!map.getLayer(circleId)) {
        map.addLayer({
          id: circleId,
          type: "circle",
          source: sourceId,
          filter: ["==", ["geometry-type"], "Point"],
          layout: { visibility: tableMapVisibility },
          paint: {
            "circle-radius": 5,
            "circle-color": "#2563eb",
            "circle-stroke-width": 1,
            "circle-stroke-color": "#fff",
            "circle-opacity": 0.8 * tableOpacity,
          },
        });
      } else {
        map.setLayoutProperty(circleId, "visibility", tableMapVisibility);
        map.setPaintProperty(circleId, "circle-opacity", 0.8 * tableOpacity);
      }

      // Auto-fit bounds to show table data
      const bounds = new maplibregl.LngLatBounds();
      let hasCoords = false;
      for (const f of fc.features) {
        if (!f.geometry) continue;
        const addCoords = (coords: number[]) => {
          bounds.extend(coords as [number, number]);
          hasCoords = true;
        };
        const walkCoords = (coords: unknown, depth: number): void => {
          if (depth === 0) addCoords(coords as number[]);
          else for (const c of coords as unknown[]) walkCoords(c, depth - 1);
        };
        const g = f.geometry as { type: string; coordinates: unknown };
        switch (g.type) {
          case "Point": walkCoords(g.coordinates, 0); break;
          case "MultiPoint":
          case "LineString": walkCoords(g.coordinates, 1); break;
          case "MultiLineString":
          case "Polygon": walkCoords(g.coordinates, 2); break;
          case "MultiPolygon": walkCoords(g.coordinates, 3); break;
        }
      }
      if (hasCoords && !bounds.isEmpty()) {
        map.fitBounds(bounds, { padding: 60, maxZoom: 15, duration: 1000 });
      }
    }

    // Remove stale table geometry layers/sources (tables that were deleted)
    const style = map.getStyle();
    if (style?.sources) {
      for (const srcId of Object.keys(style.sources)) {
        if (srcId.startsWith(TABLE_SOURCE_PREFIX) && !activeSources.has(srcId)) {
          const tName = srcId.slice(TABLE_SOURCE_PREFIX.length);
          for (const prefix of [TABLE_FILL_PREFIX, TABLE_LINE_PREFIX, TABLE_CIRCLE_PREFIX]) {
            const layerId = `${prefix}${tName}`;
            if (map.getLayer(layerId)) map.removeLayer(layerId);
          }
          map.removeSource(srcId);
        }
      }
    }
  }, [tableGeoJson, layerVisibility, layerOpacity]);

  // MapLibre layers for risk overlay GeoJSON (polygon/line/point geometries)
  useEffect(() => {
    const map = mapRef.current;
    if (!map || !map.isStyleLoaded()) return;

    const activeRiskSources = new Set<string>();

    for (const [riskName, rawFc] of Object.entries(riskLayerGeoJson)) {
      const fc = rawFc as GeoJSON.FeatureCollection;
      if (!fc.features?.length) continue;

      const riskLayerId = `risk-${riskName}`;
      const riskVisible = layerVisibility[riskLayerId] !== false;
      const riskOpacity = layerOpacity[riskLayerId] ?? 1;
      const riskMapVisibility = riskVisible ? "visible" : "none";

      const sourceId = `${RISK_SOURCE_PREFIX}${riskName}`;
      const fillId = `${RISK_FILL_PREFIX}${riskName}`;
      const lineId = `${RISK_LINE_PREFIX}${riskName}`;
      const circleId = `${RISK_CIRCLE_PREFIX}${riskName}`;
      activeRiskSources.add(sourceId);

      if (!map.getSource(sourceId)) {
        map.addSource(sourceId, { type: "geojson", data: fc });
      } else {
        (map.getSource(sourceId) as maplibregl.GeoJSONSource).setData(fc);
      }

      // Fill layer for polygons
      if (!map.getLayer(fillId)) {
        map.addLayer({
          id: fillId,
          type: "fill",
          source: sourceId,
          filter: ["in", ["geometry-type"], ["literal", ["Polygon", "MultiPolygon"]]],
          layout: { visibility: riskMapVisibility },
          paint: {
            "fill-color": "#f59e0b",
            "fill-opacity": 0.25 * riskOpacity,
          },
        });
      } else {
        map.setLayoutProperty(fillId, "visibility", riskMapVisibility);
        map.setPaintProperty(fillId, "fill-opacity", 0.25 * riskOpacity);
      }

      // Line layer for lines and polygon outlines
      if (!map.getLayer(lineId)) {
        map.addLayer({
          id: lineId,
          type: "line",
          source: sourceId,
          filter: ["in", ["geometry-type"], ["literal", ["LineString", "MultiLineString", "Polygon", "MultiPolygon"]]],
          layout: { visibility: riskMapVisibility },
          paint: {
            "line-color": "#f59e0b",
            "line-width": 1.5,
            "line-opacity": 0.7 * riskOpacity,
          },
        });
      } else {
        map.setLayoutProperty(lineId, "visibility", riskMapVisibility);
        map.setPaintProperty(lineId, "line-opacity", 0.7 * riskOpacity);
      }

      // Circle layer for points
      if (!map.getLayer(circleId)) {
        map.addLayer({
          id: circleId,
          type: "circle",
          source: sourceId,
          filter: ["==", ["geometry-type"], "Point"],
          layout: { visibility: riskMapVisibility },
          paint: {
            "circle-radius": 5,
            "circle-color": "#f59e0b",
            "circle-stroke-width": 1,
            "circle-stroke-color": "#fff",
            "circle-opacity": 0.8 * riskOpacity,
          },
        });
      } else {
        map.setLayoutProperty(circleId, "visibility", riskMapVisibility);
        map.setPaintProperty(circleId, "circle-opacity", 0.8 * riskOpacity);
      }
    }

    // Remove stale risk geometry layers/sources
    const style = map.getStyle();
    if (style?.sources) {
      for (const srcId of Object.keys(style.sources)) {
        if (srcId.startsWith(RISK_SOURCE_PREFIX) && !activeRiskSources.has(srcId)) {
          const rName = srcId.slice(RISK_SOURCE_PREFIX.length);
          for (const prefix of [RISK_FILL_PREFIX, RISK_LINE_PREFIX, RISK_CIRCLE_PREFIX]) {
            const layerId = `${prefix}${rName}`;
            if (map.getLayer(layerId)) map.removeLayer(layerId);
          }
          map.removeSource(srcId);
        }
      }
    }
  }, [riskLayerGeoJson, layerVisibility, layerOpacity]);

  async function handleExportPng() {
    if (!mapRef.current || !containerRef.current) return;
    setExporting(true);
    try {
      // Get the MapLibre canvas
      const mapCanvas = mapRef.current.getCanvas();
      // Create composite canvas
      const canvas = document.createElement("canvas");
      canvas.width = mapCanvas.width;
      canvas.height = mapCanvas.height;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      // Draw MapLibre base
      ctx.drawImage(mapCanvas, 0, 0);

      // Find and draw Deck.gl canvas (overlay canvas inside the map container)
      const allCanvases = containerRef.current.querySelectorAll("canvas");
      for (const c of allCanvases) {
        if (c !== mapCanvas && c.width > 0 && c.height > 0) {
          ctx.drawImage(c, 0, 0);
        }
      }

      const dataUrl = canvas.toDataURL("image/png");
      const filePath = await save({
        defaultPath: "map_export.png",
        filters: [{ name: "PNG Image", extensions: ["png"] }],
      });
      if (filePath) {
        await invoke("save_file", { filePath, data: dataUrl });
      }
    } catch {
      // Silently fail — non-fatal
    }
    setExporting(false);
  }

  // Truncation indicator
  const shownFeatures = analysisFeatures.length;
  const isTruncated = analysisTotalCount !== null && analysisTotalCount > shownFeatures && shownFeatures > 0;

  return (
    <div className="map-fill">
      <div ref={containerRef} style={{ width: "100%", height: "100%" }} />
      <LayerPanel />
      <BasemapSelector left={86} />
      <MapLegend />

      {/* Map export button */}
      {isTauri() && (
        <button
          onClick={() => void handleExportPng()}
          disabled={exporting}
          title="Export map as PNG"
          style={{
            position: "absolute",
            top: 10,
            left: 224,
            zIndex: 5,
            background: "rgba(15, 15, 20, 0.85)",
            backdropFilter: "blur(8px)",
            border: "1px solid rgba(148, 163, 184, 0.15)",
            borderRadius: 6,
            padding: "5px 10px",
            color: "rgba(255,255,255,0.7)",
            fontSize: 11,
            cursor: exporting ? "wait" : "pointer",
            display: "flex",
            alignItems: "center",
            gap: 4,
          }}
        >
          {/* Camera icon */}
          <svg width="13" height="13" viewBox="0 0 13 13" fill="none" aria-hidden="true">
            <path d="M1.5 4.5a1 1 0 011-1h1.5l1-1.5h3l1 1.5h1.5a1 1 0 011 1v5a1 1 0 01-1 1h-8a1 1 0 01-1-1v-5z" stroke="currentColor" strokeWidth="1.1" />
            <circle cx="6.5" cy="6.5" r="1.75" stroke="currentColor" strokeWidth="1.1" />
          </svg>
          {exporting ? "Exporting..." : "Export"}
        </button>
      )}

      {/* Truncation indicator */}
      {isTruncated && (
        <div
          style={{
            position: "absolute",
            bottom: 36,
            left: 10,
            zIndex: 5,
            background: "rgba(245, 158, 11, 0.15)",
            border: "1px solid rgba(245, 158, 11, 0.3)",
            borderRadius: 6,
            padding: "4px 10px",
            fontSize: 11,
            color: "rgba(245, 158, 11, 0.9)",
          }}
        >
          Showing {shownFeatures.toLocaleString()} of {analysisTotalCount.toLocaleString()} features
        </div>
      )}

      {!hasData && (
        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 -ml-[150px] -mt-10 z-5 pointer-events-none text-center">
          <div className="glass-panel rounded-xl px-6 py-4 shadow-lg inline-block">
            <div className="text-sm font-semibold mb-1">
              Your data will appear here
            </div>
            <div className="text-xs text-muted-foreground">
              Upload a CSV, GeoJSON, or Shapefile to plot data on the map
            </div>
          </div>
        </div>
      )}
    </div>
  );
});
