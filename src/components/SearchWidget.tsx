import { useState, useRef, type RefObject } from "react";
import maplibregl from "maplibre-gl";
import { TextField, Card, Box, Text, Flex } from "@radix-ui/themes";
import { MOCK_GEOCODE_RESULTS, type GeocodeResult } from "../mock/data";

// BLOCKER: Replace synchronous mock filter below with an async safeInvoke call:
//   import { safeInvoke } from "../lib/tauri";
//   const raw = await safeInvoke<string>(
//     "execute_engine_command",
//     { command: `geocode "${query}"` },
//     JSON.stringify(filtered),
//   );
//   const results = JSON.parse(raw ?? "[]") as GeocodeResult[];
//
//   Long-term: swap to Overture-based place search once the local DuckDB table is ready:
//     command: `overture_search ./spatia.duckdb places_wa "${query}" 10`

interface Props {
  mapRef: RefObject<maplibregl.Map | null>;
}

export function SearchWidget({ mapRef }: Props) {
  const [query, setQuery] = useState("");
  const [suggestions, setSuggestions] = useState<GeocodeResult[]>([]);
  const [open, setOpen] = useState(false);
  const markerRef = useRef<maplibregl.Marker | null>(null);

  function handleChange(e: React.ChangeEvent<HTMLInputElement>) {
    const val = e.target.value;
    setQuery(val);
    if (val.trim().length < 2) {
      setSuggestions([]);
      setOpen(false);
      return;
    }
    const filtered = MOCK_GEOCODE_RESULTS.filter((r) =>
      r.address.toLowerCase().includes(val.toLowerCase()),
    );
    setSuggestions(filtered);
    setOpen(filtered.length > 0);
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Escape") {
      setOpen(false);
    }
  }

  function selectResult(r: GeocodeResult) {
    setQuery(r.address);
    setSuggestions([]);
    setOpen(false);
    const map = mapRef.current;
    if (!map) return;
    map.flyTo({ center: [r.lon, r.lat], zoom: 13, duration: 1000 });
    if (markerRef.current) {
      markerRef.current.setLngLat([r.lon, r.lat]);
    } else {
      markerRef.current = new maplibregl.Marker({ color: "#7c3aed" })
        .setLngLat([r.lon, r.lat])
        .addTo(map);
    }
  }

  return (
    <Box className="search-widget">
      <TextField.Root
        value={query}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        onBlur={() => setOpen(false)}
        placeholder="🔍 Search places…"
        size="2"
        variant="soft"
      />
      {open && suggestions.length > 0 && (
        <Card className="search-suggestions" size="1">
          <Flex direction="column">
            {suggestions.map((r) => (
              <Box
                key={`${r.address}-${r.lat}`}
                px="3"
                py="2"
                className="search-suggestion-item"
                style={{ cursor: "pointer", borderRadius: "var(--radius-2)" }}
                onMouseDown={(e) => {
                  e.preventDefault(); // prevent input blur before click fires
                  selectResult(r);
                }}
              >
                <Text size="2" as="div">
                  {r.address}
                </Text>
                <Text size="1" color="gray">
                  {r.lat.toFixed(4)}, {r.lon.toFixed(4)}
                </Text>
              </Box>
            ))}
          </Flex>
        </Card>
      )}
    </Box>
  );
}
