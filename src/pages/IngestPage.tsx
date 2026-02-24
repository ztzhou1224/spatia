import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Box,
  Heading,
  Text,
  TextField,
  Button,
  Callout,
  Flex,
  Code,
  Strong,
} from "@radix-ui/themes";
import { safeInvoke, isTauri } from "../lib/tauri";
import { type IngestResult, MOCK_INGEST_RESULT } from "../mock/data";

type IngestProgressEvent = {
  stage: string;
  message: string;
  percent: number;
};

export function IngestPage() {
  const [dbPath, setDbPath] = useState("./spatia.duckdb");
  const [csvPath, setCsvPath] = useState("");
  const [tableName, setTableName] = useState("raw_staging");
  const [result, setResult] = useState<IngestResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<IngestProgressEvent | null>(null);

  useEffect(() => {
    if (!isTauri()) {
      return;
    }

    let unlisten: (() => void) | undefined;
    const attach = async () => {
      unlisten = await listen<IngestProgressEvent>(
        "ingest-progress",
        (event) => {
          setProgress(event.payload);
        },
      );
    };

    void attach();

    return () => {
      unlisten?.();
    };
  }, []);

  async function handlePickCsv() {
    if (!isTauri()) {
      return;
    }

    const selected = await open({
      multiple: false,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });

    if (typeof selected === "string") {
      setCsvPath(selected);
    }
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError(null);
    setResult(null);
    setProgress(null);

    let raw: string | undefined;
    if (isTauri()) {
      raw = await safeInvoke<string>("ingest_csv_with_progress", {
        dbPath,
        csvPath,
        tableName,
      });
    } else {
      setProgress({
        stage: "completed",
        message: "Demo mode: using mock ingest response",
        percent: 100,
      });
      raw = JSON.stringify({ ...MOCK_INGEST_RESULT, table: tableName });
    }

    if (!raw) {
      setError("No response from engine.");
    } else {
      try {
        setResult(JSON.parse(raw) as IngestResult);
      } catch {
        setError(`Unexpected response: ${raw}`);
      }
    }

    setLoading(false);
  }

  return (
    <Box
      p="5"
      style={{ flex: 1, minHeight: 0, overflowY: "auto", maxWidth: 520 }}
    >
      <Heading size="5" mb="2">
        Ingest CSV
      </Heading>
      <Text as="p" size="2" color="gray" mb="4">
        Load a CSV file into a DuckDB table via the engine's <Code>ingest</Code>{" "}
        command.
        {!isTauri() && (
          <Text color="red"> (Demo mode — mock result shown)</Text>
        )}
      </Text>

      <form onSubmit={handleSubmit}>
        <Flex direction="column" gap="3">
          <Box>
            <Text size="2" weight="medium" as="div" mb="1">
              Database path
            </Text>
            <TextField.Root
              value={dbPath}
              onChange={(e) => setDbPath(e.target.value)}
              placeholder="./spatia.duckdb"
              required
            />
          </Box>

          <Box>
            <Text size="2" weight="medium" as="div" mb="1">
              CSV file path
            </Text>
            <Flex gap="2">
              <Box style={{ flex: 1 }}>
                <TextField.Root
                  value={csvPath}
                  onChange={(e) => setCsvPath(e.target.value)}
                  placeholder="./data/sample.csv"
                  required
                />
              </Box>
              {isTauri() && (
                <Button type="button" variant="soft" onClick={handlePickCsv}>
                  Pick file
                </Button>
              )}
            </Flex>
          </Box>

          <Box>
            <Text size="2" weight="medium" as="div" mb="1">
              Table name
            </Text>
            <TextField.Root
              value={tableName}
              onChange={(e) => setTableName(e.target.value)}
              placeholder="raw_staging"
              required
            />
          </Box>

          <Button type="submit" disabled={loading} size="2">
            {loading ? "Ingesting…" : "Ingest"}
          </Button>
        </Flex>
      </form>

      {error && (
        <Callout.Root color="red" variant="soft" mt="4">
          <Callout.Text>{error}</Callout.Text>
        </Callout.Root>
      )}

      {loading && progress && (
        <Callout.Root color="blue" variant="soft" mt="4">
          <Callout.Text>
            <Strong>{progress.percent}%</Strong> — {progress.message}
          </Callout.Text>
        </Callout.Root>
      )}

      {result && (
        <Callout.Root color="green" variant="soft" mt="4">
          <Callout.Text>
            Table <Strong>{result.table}</Strong> loaded (status:{" "}
            {result.status}){!isTauri() && " [mock]"}
          </Callout.Text>
        </Callout.Root>
      )}
    </Box>
  );
}
