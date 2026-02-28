import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Badge,
  Box,
  Button,
  Card,
  Flex,
  Heading,
  Select,
  Spinner,
  Text,
} from "@radix-ui/themes";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "../lib/tauri";

// ---- Types ----

type FileJobStatus =
  | "queued"
  | "ingesting"
  | "cleaning"
  | "detecting"
  | "ready"
  | "geocoding"
  | "done"
  | "error";

type FileJob = {
  id: string;
  filename: string;
  csvPath: string;
  tableName: string;
  status: FileJobStatus;
  progressMessage: string;
  progressPercent: number;
  cleanSummary?: string;
  addressColumns: string[];
  geocodeColumn?: string;
  error?: string;
};

type IngestProgressPayload = {
  stage: string;
  message: string;
  percent: number;
};

type CleanProgressPayload = {
  stage: string;
  message: string;
  percent: number;
  round: number;
};

type GeocodeProgressPayload = {
  stage: string;
  message: string;
  percent: number;
};

// ---- Helpers ----

function sanitizeTableName(filename: string): string {
  // Strip directory and extension
  const base = filename.replace(/.*[\\/]/, "").replace(/\.[^.]+$/, "");
  // Lowercase and replace non-alphanumeric with _
  let name = base.toLowerCase().replace(/[^a-z0-9]+/g, "_");
  // Remove leading digits/underscores until we have a letter or underscore start
  name = name.replace(/^[0-9]+/, "");
  if (!name || name === "_") {
    name = "table_" + Date.now();
  }
  return name;
}

function statusColor(
  status: FileJobStatus,
): "gray" | "blue" | "yellow" | "green" | "red" {
  switch (status) {
    case "queued":
      return "gray";
    case "ingesting":
    case "cleaning":
    case "detecting":
    case "geocoding":
      return "blue";
    case "ready":
      return "yellow";
    case "done":
      return "green";
    case "error":
      return "red";
  }
}

function isActive(status: FileJobStatus): boolean {
  return ["ingesting", "cleaning", "detecting", "geocoding"].includes(status);
}

// ---- Component ----

export function IngestPage() {
  const [jobs, setJobs] = useState<FileJob[]>([]);
  // Selected geocode column per job id
  const [geocodeColPick, setGeocodeColPick] = useState<
    Record<string, string>
  >({});

  const currentJobIdRef = useRef<string | null>(null);
  // Reference to latest jobs so event handlers always see current state
  const jobsRef = useRef<FileJob[]>(jobs);
  useEffect(() => {
    jobsRef.current = jobs;
  }, [jobs]);

  // processQueue ref so recursive calls always point to the latest version
  const processQueueRef = useRef<(() => Promise<void>) | undefined>(undefined);

  function updateJob(id: string, patch: Partial<FileJob>) {
    setJobs((prev) =>
      prev.map((j) => (j.id === id ? { ...j, ...patch } : j)),
    );
  }

  // ---- Event listeners ----
  useEffect(() => {
    if (!isTauri()) return;

    let unlistenIngest: (() => void) | undefined;
    let unlistenClean: (() => void) | undefined;
    let unlistenGeocode: (() => void) | undefined;

    const attach = async () => {
      unlistenIngest = await listen<IngestProgressPayload>(
        "ingest-progress",
        (event) => {
          const id = currentJobIdRef.current;
          if (!id) return;
          updateJob(id, {
            progressMessage: event.payload.message,
            progressPercent: event.payload.percent,
          });
        },
      );

      unlistenClean = await listen<CleanProgressPayload>(
        "clean-progress",
        (event) => {
          const id = currentJobIdRef.current;
          if (!id) return;
          updateJob(id, {
            progressMessage: event.payload.message,
            progressPercent: event.payload.percent,
          });
        },
      );

      unlistenGeocode = await listen<GeocodeProgressPayload>(
        "geocode-progress",
        (event) => {
          const id = currentJobIdRef.current;
          if (!id) return;
          updateJob(id, {
            progressMessage: event.payload.message,
            progressPercent: event.payload.percent,
          });
        },
      );
    };

    void attach();

    return () => {
      unlistenIngest?.();
      unlistenClean?.();
      unlistenGeocode?.();
    };
  }, []);

  // ---- Pipeline processor ----

  async function processQueue() {
    const snapshot = jobsRef.current;
    const nextJob = snapshot.find((j) => j.status === "queued");
    if (!nextJob) return;

    const { id, csvPath, tableName } = nextJob;
    currentJobIdRef.current = id;

    try {
      // Step 1: ingest
      updateJob(id, {
        status: "ingesting",
        progressMessage: "Starting ingestion...",
        progressPercent: 0,
      });

      await invoke("ingest_csv_with_progress", {
        csvPath,
        tableName,
      });

      // Step 2: AI clean
      updateJob(id, {
        status: "cleaning",
        progressMessage: "Starting AI clean...",
        progressPercent: 0,
      });

      const cleanRaw = await invoke<string>("clean_table_with_progress", {
        tableName,
      });
      let cleanSummary: string | undefined;
      try {
        const cleanResult = JSON.parse(cleanRaw) as {
          status: string;
          rounds?: number;
          total_statements?: number;
          reason?: string;
        };
        if (cleanResult.status === "skipped") {
          cleanSummary = "AI clean skipped (no API key)";
        } else {
          cleanSummary = `${cleanResult.rounds ?? 0} round(s), ${cleanResult.total_statements ?? 0} statement(s)`;
        }
      } catch {
        cleanSummary = "clean complete";
      }

      // Step 3: detect address columns
      updateJob(id, {
        status: "detecting",
        progressMessage: "Detecting address columns...",
        progressPercent: 90,
        cleanSummary,
      });

      const detectRaw = await invoke<string>("detect_address_columns", {
        tableName,
      });
      let addressColumns: string[] = [];
      try {
        const detectResult = JSON.parse(detectRaw) as {
          columns: string[];
        };
        addressColumns = detectResult.columns ?? [];
      } catch {
        addressColumns = [];
      }

      updateJob(id, {
        status: addressColumns.length > 0 ? "ready" : "done",
        progressMessage: "",
        progressPercent: 100,
        addressColumns,
      });
    } catch (err) {
      updateJob(id, {
        status: "error",
        error: String(err),
        progressMessage: "",
      });
    } finally {
      currentJobIdRef.current = null;
    }

    // Process next queued job
    await processQueueRef.current?.();
  }

  processQueueRef.current = processQueue;

  // ---- File picker ----

  async function handlePickFiles() {
    if (!isTauri()) return;

    const selected = await open({
      multiple: true,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });

    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    if (paths.length === 0) return;

    const newJobs: FileJob[] = paths.map((csvPath) => {
      const filename = csvPath.replace(/.*[\\/]/, "");
      return {
        id: crypto.randomUUID(),
        filename,
        csvPath,
        tableName: sanitizeTableName(csvPath),
        status: "queued",
        progressMessage: "Waiting...",
        progressPercent: 0,
        addressColumns: [],
      };
    });

    setJobs((prev) => {
      jobsRef.current = [...prev, ...newJobs];
      return jobsRef.current;
    });

    // Kick off queue after state is set
    setTimeout(() => {
      void processQueueRef.current?.();
    }, 0);
  }

  // ---- Geocode action ----

  async function handleGeocode(job: FileJob) {
    const col = geocodeColPick[job.id] ?? job.addressColumns[0];
    if (!col) return;

    currentJobIdRef.current = job.id;
    updateJob(job.id, {
      status: "geocoding",
      progressMessage: "Starting geocoding...",
      progressPercent: 0,
      geocodeColumn: col,
    });

    try {
      await invoke("geocode_table_column", {
        tableName: job.tableName,
        addressCol: col,
      });
      updateJob(job.id, {
        status: "done",
        progressMessage: "",
        progressPercent: 100,
      });
    } catch (err) {
      updateJob(job.id, {
        status: "error",
        error: String(err),
        progressMessage: "",
      });
    } finally {
      currentJobIdRef.current = null;
    }
  }

  // ---- Delete action ----

  async function handleDelete(job: FileJob) {
    if (!confirm(`Delete table "${job.tableName}" from the database?`)) return;

    try {
      await invoke("drop_table", { tableName: job.tableName });
    } catch {
      // Ignore errors on drop — table may not exist yet
    }

    setJobs((prev) => prev.filter((j) => j.id !== job.id));
    setGeocodeColPick((prev) => {
      const next = { ...prev };
      delete next[job.id];
      return next;
    });
  }

  // ---- Render ----

  return (
    <Box p="5" style={{ flex: 1, minHeight: 0, overflowY: "auto", maxWidth: 640 }}>
      <Flex align="center" justify="between" mb="4">
        <Heading size="5">Upload Data</Heading>
        {isTauri() && (
          <Button onClick={() => void handlePickFiles()} size="2">
            Select files
          </Button>
        )}
        {!isTauri() && (
          <Text size="2" color="red">
            Demo mode — Tauri required
          </Text>
        )}
      </Flex>

      {jobs.length === 0 && (
        <Text as="p" size="2" color="gray">
          Pick one or more CSV files to ingest, clean, and optionally geocode.
        </Text>
      )}

      <Flex direction="column" gap="3">
        {jobs.map((job) => (
          <FileJobCard
            key={job.id}
            job={job}
            selectedCol={geocodeColPick[job.id] ?? job.addressColumns[0] ?? ""}
            onColChange={(col) =>
              setGeocodeColPick((prev) => ({ ...prev, [job.id]: col }))
            }
            onGeocode={() => void handleGeocode(job)}
            onDelete={() => void handleDelete(job)}
          />
        ))}
      </Flex>
    </Box>
  );
}

// ---- FileJobCard sub-component ----

type FileJobCardProps = {
  job: FileJob;
  selectedCol: string;
  onColChange: (col: string) => void;
  onGeocode: () => void;
  onDelete: () => void;
};

function FileJobCard({
  job,
  selectedCol,
  onColChange,
  onGeocode,
  onDelete,
}: FileJobCardProps) {
  const active = isActive(job.status);

  return (
    <Card>
      <Flex direction="column" gap="2">
        {/* Header row */}
        <Flex align="center" justify="between" gap="2">
          <Flex direction="column" gap="1" style={{ flex: 1, minWidth: 0 }}>
            <Text size="2" weight="medium" style={{ wordBreak: "break-all" }}>
              {job.filename}
            </Text>
            <Text size="1" color="gray">
              → {job.tableName}
            </Text>
          </Flex>
          <Flex align="center" gap="2" flexShrink="0">
            {active && <Spinner size="1" />}
            <Badge color={statusColor(job.status)} size="1">
              {job.status}
            </Badge>
          </Flex>
        </Flex>

        {/* Progress bar */}
        {active && job.progressMessage && (
          <Box>
            <Box
              style={{
                height: 4,
                background: "var(--gray-4)",
                borderRadius: 2,
                overflow: "hidden",
              }}
            >
              <Box
                style={{
                  height: "100%",
                  width: `${job.progressPercent}%`,
                  background: "var(--accent-9)",
                  transition: "width 0.3s ease",
                }}
              />
            </Box>
            <Text size="1" color="gray" mt="1">
              {job.progressPercent}% — {job.progressMessage}
            </Text>
          </Box>
        )}

        {/* Clean summary */}
        {job.cleanSummary && !active && (
          <Text size="1" color="gray">
            AI clean: {job.cleanSummary}
          </Text>
        )}

        {/* Error */}
        {job.status === "error" && job.error && (
          <Text size="1" color="red">
            Error: {job.error}
          </Text>
        )}

        {/* Geocode section — shown when address columns exist and job is ready/done */}
        {(job.status === "ready" || job.status === "done") &&
          job.addressColumns.length > 0 && (
            <Flex align="center" gap="2" wrap="wrap">
              <Text size="2">Address column:</Text>
              {job.addressColumns.length === 1 ? (
                <Text size="2" weight="medium">
                  {job.addressColumns[0]}
                </Text>
              ) : (
                <Select.Root
                  value={selectedCol}
                  onValueChange={onColChange}
                  size="1"
                >
                  <Select.Trigger />
                  <Select.Content>
                    {job.addressColumns.map((col) => (
                      <Select.Item key={col} value={col}>
                        {col}
                      </Select.Item>
                    ))}
                  </Select.Content>
                </Select.Root>
              )}
              {job.status === "ready" && (
                <Button size="1" variant="soft" onClick={onGeocode}>
                  Geocode
                </Button>
              )}
              {job.status === "done" && job.geocodeColumn && (
                <Text size="1" color="green">
                  Geocoded via "{job.geocodeColumn}"
                </Text>
              )}
            </Flex>
          )}

        {/* Delete button */}
        <Flex justify="end">
          <Button
            size="1"
            variant="ghost"
            color="red"
            onClick={onDelete}
            disabled={active}
          >
            Delete
          </Button>
        </Flex>
      </Flex>
    </Card>
  );
}
