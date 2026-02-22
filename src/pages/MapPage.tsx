import { useEffect, useRef } from "react";
import maplibregl from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";

// BLOCKER: Replace OSM raster tiles with PMTiles vector layers once the engine's
//   overture_extract + PMTiles precompute workflow produces tile artifacts.
//   Use @maplibre/maplibre-gl-pmtiles to add vector sources:
//     invoke("execute_engine_command", { command: "overture_extract ..." })
//   will return the local PMTiles path; load it as a protocol source here.
const OSM_STYLE: maplibregl.StyleSpecification = {
  version: 8,
  sources: {
    osm: {
      type: "raster",
      tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
      tileSize: 256,
      attribution:
        "© <a href='https://www.openstreetmap.org/copyright'>OpenStreetMap</a> contributors",
    },
  },
  layers: [
    {
      id: "osm-tiles",
      type: "raster",
      source: "osm",
    },
  ],
};

export function MapPage() {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);

  useEffect(() => {
    if (mapRef.current || !containerRef.current) return;

    mapRef.current = new maplibregl.Map({
      container: containerRef.current,
      style: OSM_STYLE,
      center: [-122.4194, 37.7749], // San Francisco – placeholder location
      zoom: 11,
    });

    mapRef.current.addControl(new maplibregl.NavigationControl(), "top-right");
    mapRef.current.addControl(
      new maplibregl.ScaleControl({ unit: "metric" }),
      "bottom-left",
    );

    return () => {
      mapRef.current?.remove();
      mapRef.current = null;
    };
  }, []);

  return (
    <div className="map-page">
      <div ref={containerRef} className="map-container" />

      {/* Layer panel – wire to real PMTiles/DuckDB layers once backend is ready */}
      <aside className="map-panel">
        <h3>Layers</h3>
        <p className="placeholder-note">
          {/*
           * BLOCKER: After an overture_extract run, load the resulting PMTiles
           * file here using the @maplibre/maplibre-gl-pmtiles protocol and add
           * vector tile layers to the map instance (mapRef.current).
           */}
          No layers loaded yet. Run <em>Ingest</em> or an Overture extract,
          then add the output PMTiles here.
        </p>
        <ul className="layer-list">
          <li className="layer-item layer-item--placeholder">
            <input type="checkbox" id="layer-places" disabled />
            <label htmlFor="layer-places">places (not loaded)</label>
          </li>
        </ul>
      </aside>
    </div>
  );
}
