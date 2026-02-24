import { useState } from "react";
import { Box, Button, Card, Flex, Text, TextArea } from "@radix-ui/themes";
import { safeInvoke, isTauri } from "../lib/tauri";
import { useFocusGuard } from "../lib/useFocusGuard";
import { buildAIContext } from "../lib/aiContext";
import { useWidgetStore } from "../lib/widgetStore";

type Message = {
  role: "user" | "assistant";
  text: string;
};

type AnalysisChatPayload = {
  assistant: string;
  system_prompt: string;
};

type Props = {
  dbPath: string;
  tableName: string;
  onGeoJson: (geojson: unknown) => void;
  onVisualizationCommand: (command: string) => void;
};

export function AnalysisChat({
  dbPath,
  tableName,
  onGeoJson,
  onVisualizationCommand,
}: Props) {
  const appFocusedWidgetId = useWidgetStore(
    (state) => state.appFocusedWidgetId,
  );
  const lastNonChatFocusedWidgetId = useWidgetStore(
    (state) => state.lastNonChatFocusedWidgetId,
  );
  const widgets = useWidgetStore((state) => state.widgets);
  const chatFocusGuard = useFocusGuard({
    id: "analysis-chat-widget",
    label: "Analysis Chat",
    kind: "chat",
  });

  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Message[]>([]);
  const [generatedSql, setGeneratedSql] = useState("");
  const [visualizationCommand, setVisualizationCommand] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSend() {
    const text = input.trim();
    if (!text || loading) return;

    const context = buildAIContext({
      widgets,
      lastNonChatFocusedWidgetId,
    });

    setLoading(true);
    setError(null);
    setMessages((prev) => [...prev, { role: "user", text }]);
    setInput("");

    const fallback = JSON.stringify({
      assistant:
        "Demo mode: schema-aware chat is available in Tauri. Run `pnpm tauri dev` to use backend AI analysis.",
      system_prompt: "",
    });

    const raw = await safeInvoke<string>(
      "analysis_chat",
      {
        dbPath,
        tableName,
        userMessage: `${text}\n\nContext:\n${context}`,
      },
      fallback,
    );

    if (!raw) {
      setError("No response from analysis chat backend.");
      setLoading(false);
      return;
    }

    try {
      const parsed = JSON.parse(raw) as AnalysisChatPayload;
      setMessages((prev) => [
        ...prev,
        { role: "assistant", text: parsed.assistant },
      ]);
    } catch {
      setError("Unexpected chat response format.");
    }

    setLoading(false);
  }

  async function handleGenerateSql() {
    const goal = input.trim();
    if (!goal || loading) return;

    setLoading(true);
    setError(null);

    const fallback = JSON.stringify({
      sql: `CREATE OR REPLACE VIEW analysis_result AS SELECT * FROM ${tableName} LIMIT 100;`,
    });

    const raw = await safeInvoke<string>(
      "generate_analysis_sql",
      {
        dbPath,
        tableName,
        userGoal: goal,
      },
      fallback,
    );

    if (!raw) {
      setError("No response from SQL generation backend.");
      setLoading(false);
      return;
    }

    try {
      const parsed = JSON.parse(raw) as { sql: string };
      setGeneratedSql(parsed.sql);
    } catch {
      setError("Unexpected SQL response format.");
    }

    setLoading(false);
  }

  async function handleExecuteSql() {
    if (!generatedSql.trim() || loading) return;

    setLoading(true);
    setError(null);

    const fallback = JSON.stringify({
      status: "ok",
      row_count: 0,
      geojson: {
        type: "FeatureCollection",
        features: [],
      },
    });

    const raw = await safeInvoke<string>(
      "execute_analysis_sql",
      {
        dbPath,
        sql: generatedSql,
      },
      fallback,
    );

    if (!raw) {
      setError("No response from SQL execution backend.");
      setLoading(false);
      return;
    }

    try {
      const parsed = JSON.parse(raw) as {
        status: string;
        row_count: number;
        geojson: unknown;
      };
      onGeoJson(parsed.geojson);
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          text: `analysis_result executed (${parsed.row_count} rows)`,
        },
      ]);
    } catch {
      setError("Unexpected SQL execution response format.");
    }

    setLoading(false);
  }

  async function handleSuggestVisualization() {
    const goal = input.trim();
    if (!goal || loading) return;

    setLoading(true);
    setError(null);

    const fallback = JSON.stringify({ visualization: "scatter" });
    const raw = await safeInvoke<string>(
      "generate_visualization_command",
      {
        tableName,
        userGoal: goal,
      },
      fallback,
    );

    if (!raw) {
      setError("No response from visualization command backend.");
      setLoading(false);
      return;
    }

    try {
      const parsed = JSON.parse(raw) as { visualization: string };
      setVisualizationCommand(parsed.visualization);
      onVisualizationCommand(parsed.visualization);
    } catch {
      setError("Unexpected visualization command format.");
    }

    setLoading(false);
  }

  const contextWidget =
    lastNonChatFocusedWidgetId && widgets[lastNonChatFocusedWidgetId]
      ? widgets[lastNonChatFocusedWidgetId]
      : null;

  return (
    <Box
      mt="3"
      {...chatFocusGuard}
      className={
        appFocusedWidgetId === "analysis-chat-widget"
          ? "widget-focus-ring"
          : undefined
      }
    >
      <Text size="2" weight="bold" as="div" mb="2">
        Analysis Chat
      </Text>

      <Card size="1" mb="2">
        <Text size="1" color="gray" as="div">
          Context
        </Text>
        <Text size="1">
          {contextWidget
            ? `${contextWidget.label} · ${buildAIContext({
                widgets,
                lastNonChatFocusedWidgetId,
              })}`
            : "No focused widget context"}
        </Text>
      </Card>

      <Card size="1" style={{ maxHeight: 180, overflowY: "auto" }}>
        <Flex direction="column" gap="2">
          {messages.length === 0 && (
            <Text size="1" color="gray">
              Ask a question about your DuckDB table. Schema context is sent
              automatically.
              {!isTauri() && " (Demo mode)"}
            </Text>
          )}

          {messages.map((message, index) => (
            <Box key={`${message.role}-${index}`}>
              <Text size="1" color="gray" as="div">
                {message.role === "user" ? "You" : "Assistant"}
              </Text>
              <Text size="2">{message.text}</Text>
            </Box>
          ))}
        </Flex>
      </Card>

      <TextArea
        value={input}
        onChange={(event) => setInput(event.target.value)}
        placeholder="Ask about the current table…"
        mt="2"
      />

      <Flex justify="between" align="center" mt="2">
        <Text size="1" color="gray">
          Context: {tableName}
        </Text>
        <Flex gap="2">
          <Button
            variant="soft"
            onClick={handleSuggestVisualization}
            disabled={loading || input.trim().length === 0}
          >
            {loading ? "Suggesting…" : "Suggest Viz"}
          </Button>
          <Button
            variant="soft"
            onClick={handleGenerateSql}
            disabled={loading || input.trim().length === 0}
          >
            {loading ? "Generating…" : "Generate SQL"}
          </Button>
          <Button
            onClick={handleSend}
            disabled={loading || input.trim().length === 0}
          >
            {loading ? "Sending…" : "Send"}
          </Button>
        </Flex>
      </Flex>

      {generatedSql && (
        <Card size="1" mt="2">
          <Text size="1" color="gray" as="div" mb="1">
            Generated SQL
          </Text>
          <Text size="1" style={{ whiteSpace: "pre-wrap" }}>
            {generatedSql}
          </Text>
          <Flex justify="end" mt="2">
            <Button onClick={handleExecuteSql} disabled={loading}>
              {loading ? "Running…" : "Run SQL"}
            </Button>
          </Flex>
        </Card>
      )}

      {visualizationCommand && (
        <Card size="1" mt="2">
          <Text size="1" color="gray" as="div" mb="1">
            Visualization Command
          </Text>
          <Text size="1">{`{"visualization":"${visualizationCommand}"}`}</Text>
        </Card>
      )}

      {error && (
        <Text size="1" color="red" mt="2" as="div">
          {error}
        </Text>
      )}
    </Box>
  );
}
