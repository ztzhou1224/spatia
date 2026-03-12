import maplibregl from "maplibre-gl";

type MapAction = {
  type: string;
  center?: [number, number];
  zoom?: number;
  bounds?: [[number, number], [number, number]];
  coordinates?: [number, number];
  text?: string;
  ids?: string[];
};

export function executeMapActions(
  map: maplibregl.Map,
  actions: unknown[]
): void {
  for (const raw of actions) {
    const action = raw as MapAction;

    switch (action.type) {
      case "fly_to":
        if (action.center) {
          map.flyTo({
            center: action.center,
            zoom: action.zoom ?? 12,
            duration: 1500,
          });
        }
        break;

      case "fit_bounds":
        if (action.bounds) {
          map.fitBounds(action.bounds, { padding: 50, duration: 1500 });
        }
        break;

      case "show_popup":
        if (action.coordinates && action.text) {
          new maplibregl.Popup()
            .setLngLat(action.coordinates)
            .setHTML(action.text)
            .addTo(map);
        }
        break;

      case "highlight_features":
        // Highlight support via filter - handled by MapView state
        break;
    }
  }
}
