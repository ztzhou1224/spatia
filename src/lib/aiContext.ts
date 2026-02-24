import { type Widget, type WidgetMetadata } from "./widgetStore";

type WidgetStoreSnapshot = {
  widgets: Record<string, Widget>;
  lastNonChatFocusedWidgetId: string | null;
};

function formatMetadata(metadata: WidgetMetadata): string[] {
  const parts: string[] = [];

  if (metadata.center) {
    parts.push(
      `center=${metadata.center[0].toFixed(4)},${metadata.center[1].toFixed(4)}`,
    );
  }
  if (metadata.zoom !== undefined) {
    parts.push(`zoom=${metadata.zoom.toFixed(2)}`);
  }
  if (metadata.bearing !== undefined) {
    parts.push(`bearing=${metadata.bearing.toFixed(1)}`);
  }
  if (metadata.pitch !== undefined) {
    parts.push(`pitch=${metadata.pitch.toFixed(1)}`);
  }
  if (metadata.activePopups !== undefined) {
    parts.push(`activePopups=${metadata.activePopups}`);
  }
  if (metadata.selectedFeatures) {
    parts.push(`selectedFeatures=${metadata.selectedFeatures.length}`);
  }
  if (metadata.visibleLayers) {
    parts.push(`visibleLayers=${metadata.visibleLayers.join(",")}`);
  }

  return parts;
}

export function buildAIContext(store: WidgetStoreSnapshot): string {
  const widgetId = store.lastNonChatFocusedWidgetId;
  if (!widgetId) {
    return "No focused widget context available.";
  }

  const widget = store.widgets[widgetId];
  if (!widget) {
    return "No focused widget context available.";
  }

  const metadata = formatMetadata(widget.metadata);
  const metadataText =
    metadata.length > 0 ? metadata.join("; ") : "no metadata";

  return `Focused widget: ${widget.label} (${widget.kind}) | ${metadataText}`;
}
