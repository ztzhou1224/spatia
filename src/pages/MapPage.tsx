import { useEffect, useRef } from "react";
import maplibregl from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import { Box, Text, Flex, Checkbox } from "@radix-ui/themes";
import { SearchWidget } from "../components/SearchWidget";

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
    <div className="map-wrapper">
      <div ref={containerRef} className="map-container" />

      <SearchWidget mapRef={mapRef} />

      {/* Layer panel – wire to real PMTiles/DuckDB layers once backend is ready */}
      {/*
       * BLOCKER: After an overture_extract run, load the resulting PMTiles
       * file here using the @maplibre/maplibre-gl-pmtiles protocol and add
       * vector tile layers to the map instance (mapRef.current).
       */}
      <Box
        style={{
          width: 220,
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
        <Text size="1" color="gray" as="p" mb="3">
          No layers loaded yet. Run <em>Ingest</em> or an Overture extract,
          then add the output PMTiles here.
        </Text>
        <Flex align="center" gap="2" style={{ opacity: 0.5 }}>
          <Checkbox disabled />
          <Text size="2" color="gray">
            places (not loaded)
          </Text>
        </Flex>
      </Box>
    </div>
  );
}
