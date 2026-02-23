import { useState, useEffect } from "react";
import {
  Box,
  Heading,
  Text,
  TextField,
  Button,
  Callout,
  Flex,
  Code,
  Table,
  Badge,
} from "@radix-ui/themes";
import { safeInvoke, isTauri } from "../lib/tauri";
import { MOCK_TABLES, type TableColumn } from "../mock/data";

export function SchemaPage() {
  const [dbPath, setDbPath] = useState("./spatia.duckdb");
  const [tableName, setTableName] = useState("places");
  const [columns, setColumns] = useState<TableColumn[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load mock schema on first render so the page isn't empty.
  useEffect(() => {
    void loadSchema();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function loadSchema() {
    setLoading(true);
    setError(null);

    const mockTable = MOCK_TABLES.find((t) => t.name === tableName);
    const mockFallback = JSON.stringify(mockTable?.columns ?? []);

    // BLOCKER: Requires Tauri backend.
    //   Real call: invoke("execute_engine_command", {
    //     command: `schema ${dbPath} ${tableName}`
    //   })
    //   Returns JSON string: array of { name: string, type: string, nullable: boolean }
    //   See engine schema.rs / executor.rs for the full schema command surface.
    const raw = await safeInvoke<string>(
      "execute_engine_command",
      { command: `schema ${dbPath} ${tableName}` },
      mockFallback,
    );

    if (!raw) {
      setError("No response from engine.");
    } else {
      try {
        setColumns(JSON.parse(raw) as TableColumn[]);
      } catch {
        setError(`Unexpected response: ${raw}`);
      }
    }

    setLoading(false);
  }

  return (
    <Box
      p="5"
      style={{ flex: 1, minHeight: 0, overflowY: "auto", maxWidth: 640 }}
    >
      <Heading size="5" mb="2">
        Schema Viewer
      </Heading>
      <Text as="p" size="2" color="gray" mb="4">
        Inspect the columns of a DuckDB table via the engine's{" "}
        <Code>schema</Code> command.
        {!isTauri() && (
          <Text color="red">
            {" "}
            (Demo mode — mock schema for <em>{tableName}</em>)
          </Text>
        )}
      </Text>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          void loadSchema();
        }}
      >
        <Flex gap="3" align="end" mb="4" wrap="wrap">
          <Box style={{ flex: 1, minWidth: 160 }}>
            <Text size="2" weight="medium" as="div" mb="1">
              Database path
            </Text>
            <TextField.Root
              value={dbPath}
              onChange={(e) => setDbPath(e.target.value)}
              placeholder="./spatia.duckdb"
            />
          </Box>
          <Box style={{ flex: 1, minWidth: 120 }}>
            <Text size="2" weight="medium" as="div" mb="1">
              Table name
            </Text>
            <TextField.Root
              value={tableName}
              onChange={(e) => setTableName(e.target.value)}
              placeholder="places"
            />
          </Box>
          <Button type="submit" disabled={loading} size="2">
            {loading ? "Loading…" : "Load schema"}
          </Button>
        </Flex>
      </form>

      {error && (
        <Callout.Root color="red" variant="soft" mb="3">
          <Callout.Text>{error}</Callout.Text>
        </Callout.Root>
      )}

      {columns && columns.length === 0 && (
        <Text size="2" color="gray">
          No columns found for table "{tableName}".
        </Text>
      )}

      {columns && columns.length > 0 && (
        <Table.Root variant="surface">
          <Table.Header>
            <Table.Row>
              <Table.ColumnHeaderCell>Column</Table.ColumnHeaderCell>
              <Table.ColumnHeaderCell>Type</Table.ColumnHeaderCell>
              <Table.ColumnHeaderCell>Nullable</Table.ColumnHeaderCell>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {columns.map((col) => (
              <Table.Row key={col.name}>
                <Table.Cell>{col.name}</Table.Cell>
                <Table.Cell>
                  <Code variant="soft">{col.type}</Code>
                </Table.Cell>
                <Table.Cell>
                  <Badge
                    color={col.nullable ? "gray" : "green"}
                    variant="soft"
                    size="1"
                  >
                    {col.nullable ? "nullable" : "not null"}
                  </Badge>
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Root>
      )}
    </Box>
  );
}
