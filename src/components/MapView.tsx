import { useEffect, useRef, useImperativeHandle, forwardRef } from "react";
import { MapboxOverlay } from "@deck.gl/mapbox";
import { ScatterplotLayer } from "@deck.gl/layers";
import { HeatmapLayer, HexagonLayer } from "@deck.gl/aggregation-layers";
import maplibregl from "maplibre-gl";
// maplibre-gl.css is imported in App.css (after Tailwind) to ensure correct cascade priority
import { useAppStore } from "../lib/appStore";

/** Base map style — always shows CartoDB dark tiles. PMTiles overlays are added
 *  dynamically at runtime only when the files are confirmed to exist. */
const BASE_STYLE: maplibregl.StyleSpecification = {
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
  layers: [
    {
      id: "basemap",
      type: "raster",
      source: "basemap",
    },
  ],
};

const ANALYSIS_SOURCE_ID = "analysis-result-source";
const ANALYSIS_LAYER_ID = "analysis-result-circles";
const ANALYSIS_FILL_LAYER_ID = "analysis-result-fill";
const ANALYSIS_LINE_LAYER_ID = "analysis-result-line";

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

  // Show welcome overlay when there are no tables and no analysis results
  const analysisFeatures = (analysisGeoJson as { features?: unknown[] })?.features ?? [];
  const hasData = tables.length > 0 || Object.keys(tableGeoJson).length > 0 || analysisFeatures.length > 0;

  const style = BASE_STYLE;

  useImperativeHandle(ref, () => ({
    getMap: () => mapRef.current,
  }));

  useEffect(() => {
    if (mapRef.current || !containerRef.current) return;

    mapRef.current = new maplibregl.Map({
      container: containerRef.current,
      style,
      center: [-122.4194, 37.7749],
      zoom: 11,
    });

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

    return () => {
      if (deckOverlayRef.current) {
        mapRef.current?.removeControl(deckOverlayRef.current);
        deckOverlayRef.current = null;
      }
      mapRef.current?.remove();
      mapRef.current = null;
    };
  }, [style]);

  // Update analysis GeoJSON layers
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;

    const applyAnalysisLayer = () => {
      const fc = analysisGeoJson as any;

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
        paint: {
          "circle-radius": 6,
          "circle-color": "#7c3aed",
          "circle-stroke-width": 1,
          "circle-stroke-color": "#fff",
          "circle-opacity": 0.8,
        },
      });

      // Add fill layer for polygons
      map.addLayer({
        id: ANALYSIS_FILL_LAYER_ID,
        type: "fill",
        source: ANALYSIS_SOURCE_ID,
        filter: ["in", ["geometry-type"], ["literal", ["Polygon", "MultiPolygon"]]],
        paint: {
          "fill-color": "#7c3aed",
          "fill-opacity": 0.3,
        },
      });

      // Add line layer for lines
      map.addLayer({
        id: ANALYSIS_LINE_LAYER_ID,
        type: "line",
        source: ANALYSIS_SOURCE_ID,
        filter: ["in", ["geometry-type"], ["literal", ["LineString", "MultiLineString"]]],
        paint: {
          "line-color": "#7c3aed",
          "line-width": 2,
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
    };

    if (map.isStyleLoaded()) {
      applyAnalysisLayer();
    } else {
      map.once("load", applyAnalysisLayer);
    }
  }, [analysisGeoJson]);

  // Deck.gl overlay — table data (blue) rendered below analysis results (purple/heatmap/hexbin)
  useEffect(() => {
    if (!deckOverlayRef.current) return;

    const layers = [];

    // Table data layers (blue) — one ScatterplotLayer per table, rendered first (bottom)
    for (const [tableName, rawFc] of Object.entries(tableGeoJson)) {
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
        layers.push(
          new ScatterplotLayer({
            id: `table-scatter-${tableName}`,
            data: pointData,
            getPosition: (d: { position: [number, number] }) => d.position,
            getRadius: 40,
            getFillColor: TABLE_POINT_COLOR,
            pickable: true,
          })
        );
      }
    }

    // Analysis result layer — rendered on top, type determined by visualizationType
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
              [84, 42, 143, 80],
              [107, 52, 168, 150],
              [124, 58, 237, 200],
              [167, 139, 250, 220],
              [221, 214, 254, 255],
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
              [63, 0, 125, 80],
              [84, 42, 143, 130],
              [107, 52, 168, 170],
              [124, 58, 237, 200],
              [167, 139, 250, 220],
              [221, 214, 254, 255],
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
            getFillColor: [124, 58, 237, 180],
            pickable: true,
          })
        );
      }
    }

    deckOverlayRef.current.setProps({ layers });
  }, [analysisGeoJson, visualizationType, tableGeoJson]);

  return (
    <div className="map-fill">
      <div ref={containerRef} style={{ width: "100%", height: "100%" }} />
      {!hasData && (
        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 -ml-[150px] -mt-10 z-5 pointer-events-none text-center">
          <div className="glass-panel rounded-xl px-6 py-4 shadow-lg inline-block">
            <div className="text-sm font-semibold mb-1">
              Your data will appear here
            </div>
            <div className="text-xs text-muted-foreground">
              Upload a CSV to plot locations on the map
            </div>
          </div>
        </div>
      )}
    </div>
  );
});
