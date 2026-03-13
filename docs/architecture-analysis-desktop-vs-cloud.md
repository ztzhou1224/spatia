# Architecture Analysis: Desktop vs Cloud for Spatia

## Context

Spatia was built as a desktop app (Tauri + React + Rust/DuckDB) on the principle that users run data locally for safety and compliance. However, two external API dependencies exist today — Gemini (AI analysis) and Geocodio (address geocoding) — which break the "fully local" promise. The founder wants:
1. An honest assessment of the current local-first value proposition
2. Whether the desktop architecture can handle enterprise data sources (BigQuery, Tableau, Oracle, Databricks, Amazon Redshift)
3. Whether moving to cloud SaaS would be better

**Deliverable**: Advisory analysis (no code changes).

---

## 1. Current Local-First Assessment

### What stays local (genuinely)
- All raw CSV data (ingested into local DuckDB file)
- All SQL execution (runs locally in DuckDB)
- Geocode cache, Overture lookup tables, Tantivy search indexes
- All frontend state (Zustand, zero external sync)

### What leaves the machine

| Destination | Data Sent | Risk Level |
|-------------|-----------|------------|
| **Gemini API** | Table schemas + up to 20 sample values per column + user questions + 10-turn conversation history + failed SQL + error messages | **HIGH** — sample values are real data. If columns are `ssn`, `patient_name`, `salary`, actual values are transmitted to Google. |
| **Geocodio API** | Raw address strings from user CSVs (batched, default 100/request) | **MEDIUM** — addresses are PII. May violate HIPAA/GDPR without a BAA. |
| **Overture S3** | Bounding box coordinates (implicit in parquet download) | **LOW** — reveals area of interest, no user data. |

### Honest verdict

**Spatia is "local-compute, cloud-assisted"** — not air-gapped, but meaningfully better than any SaaS GIS tool. Raw datasets never leave the machine. A user who never uses AI and has pre-existing lat/lon columns has a fully local experience.

For compliance: schema metadata + 20 sample values + address strings almost certainly constitute regulated data under HIPAA and usually under GDPR. Spatia is **not compliant** for health data or EU personal data without additional safeguards.

**Comparison to SaaS**: Spatia wins on every dimension (data residency, breach blast radius, who can access data at rest, audit trail) except "fully air-gapped."

---

## 2. Desktop Path: Enterprise Data Sources

### Feasibility Matrix

| Source | DuckDB Extension Available | Desktop Feasibility | Key Blocker |
|--------|---------------------------|--------------------|----|
| **Redshift** | `postgres_scanner` (mature) | **Medium-High** | Best option. VPN still needed. |
| **Databricks** | `httpfs` + experimental `delta` | **Medium** | Delta Lake tables readable via parquet. PATs work from desktop. |
| **BigQuery** | None native; possible via GCS parquet export | **Medium** | Requires GCP service account JSON. Must export to parquet first. |
| **Oracle** | None native; ODBC possible | **Low-Medium** | Oracle Instant Client (~300MB). ODBC on macOS/Windows is brittle. |
| **Tableau** | None | **Low** | `.twbx` is proprietary. OAuth required for Server/Cloud. |

### Structural barriers for desktop + enterprise data
1. **VPN/Firewall**: Enterprise databases are behind corporate firewalls. Desktop apps can't reach them without VPN, SSH tunnels, or a cloud proxy.
2. **Credential management**: Current env vars work for 2 API keys. Enterprise sources need OAuth, service accounts, certificates → need an OS keychain integration.
3. **Data volume**: Current architecture has a single DuckDB file, no connection pooling, 1000-row GeoJSON limit. Enterprise sources can be TB-scale → need pushdown queries and materialized subsets.
4. **Bundle size**: Each DuckDB extension adds 10-30MB. Already at ~100-150MB per platform.

**Verdict**: Viable for Redshift and Databricks with 4-8 weeks work. Marginal for BigQuery. Poor for Oracle and Tableau. The VPN/firewall problem is the structural barrier.

---

## 3. Cloud SaaS Path

### What you gain
1. **Enterprise connectivity** — server in customer's VPC reaches BigQuery, Redshift, etc. without VPN gymnastics
2. **Multi-user collaboration** — desktop Spatia is single-user (single DuckDB file, no concurrent writes)
3. **Scale beyond single-machine limits** — cloud backend can use 256GB+ RAM instances
4. **Distribution simplicity** — no per-platform builds, no auto-update, ship a URL
5. **AI key management** — users don't need their own Gemini API key

### What you lose
1. **The core value proposition** — "your data stays on your machine" is gone. You become Carto/Felt with a different UI.
2. **Operational simplicity** — SaaS requires infrastructure, auth, multi-tenancy, billing, uptime SLAs, SOC2, data residency. At least 2-3 additional hires.
3. **Cost structure** — desktop marginal cost per user is $0. SaaS at 1000 users: ~$10-30K/month in cloud costs before salaries.
4. **Offline capability** — gone.

### Migration effort: ~14-19 weeks
- Frontend (React + MapLibre + Deck.gl) is already web-native — minimal change
- Replace Tauri IPC with REST/WebSocket API: 3-4 weeks
- Server-side DuckDB per-tenant: 2-3 weeks
- Auth/multi-tenancy: 3-4 weeks
- Infrastructure (Terraform + CI/CD + monitoring): 3-4 weeks
- Upload-based file ingestion: 1-2 weeks
- WebSocket/SSE for progress events: 1 week

**Verdict**: Solves the enterprise data source problem but destroys the core product identity. Competing with Carto, Felt, ArcGIS Online on their turf, with their funding and head start.

---

## 4. Hybrid Option: Desktop + Local LLM + Bridge Service

### Architecture

```
User's Machine                          Customer's VPC (optional)
+----------------------------+          +----------------------------+
|  Spatia Desktop App        |          |  Spatia Bridge Service     |
|  +----------------------+  |   mTLS   |  (lightweight Rust binary) |
|  | React + MapLibre     |  |          |                            |
|  | Tauri shell          |<-|--------->|  - Proxies queries to      |
|  | DuckDB (local)       |  |          |    BigQuery/Redshift/etc.  |
|  | Local LLM (optional) |  |          |  - Returns result sets     |
|  +----------------------+  |          |  - No data stored           |
+----------------------------+          +----------------------------+
```

### Component 1: Local LLM for Air-Gapped AI (3-4 weeks)
- Add `LlmBackend` trait with `GeminiBackend` (existing) and `LocalBackend` (new)
- The `GeminiClient::generate()` abstraction boundary in `client.rs` is clean — this is well-bounded
- Model options: Qwen2.5-Coder 7B Q4 (~4GB), CodeLlama 13B Q4 (~8GB)
- Ship as "Local AI (Beta)" with Gemini as default, local as opt-in for compliance users
- Tradeoff: slower (5-30s CPU vs 1-3s Gemini), less accurate for complex multi-step SQL

### Component 2: Enhanced Local Geocoding (1-2 weeks)
- Add Overture `addresses` theme extraction (currently only `places`)
- Dramatically improves local hit rate, reducing Geocodio fallback
- The `overture_extract_to_table` flow already supports multiple themes

### Component 3: Bridge Service for Enterprise Data (4-6 weeks)
- Ship a lightweight Rust binary (`spatia_bridge` crate) using `axum`
- Customer deploys in their VPC — **not a Spatia-hosted service**
- Stateless query proxy: receives query, executes against enterprise source, returns Arrow IPC or Parquet
- Desktop Spatia ingests result set into local DuckDB
- Preserves "your data, your infrastructure" promise
- Start with Redshift (`postgres_scanner` protocol) + Databricks (Delta/parquet)

### Component 4: Fix the local-first story (1-2 weeks)
- Add `--local-only` mode that disables all external API calls
- Add PII column detection: warn before sending samples to Gemini if column name matches `ssn`, `dob`, `salary`, `patient`, etc.

---

## 5. Recommendation

### Decision Matrix

| Path | Enterprise Data | Privacy Story | Engineering Cost | Competitive Position |
|------|----------------|--------------|-----------------|---------------------|
| Desktop only (current) | Weak | Good (with caveats) | Lowest | Unique but limited TAM |
| **Desktop + Local LLM + Bridge** | **Strong** | **Excellent** | **Medium (10-14 weeks)** | **Strongly differentiated** |
| Full SaaS conversion | Strong | Destroyed | High (14-19 weeks + ongoing ops) | Undifferentiated |

### Recommended path: Desktop + Local LLM + Bridge

**Priority order:**
1. **Fix local-first story** (2-4 weeks) — PII detection, `--local-only` mode, expand Overture addresses
2. **Build local LLM option** (3-4 weeks) — "AI-powered GIS that never sends data to the cloud" is a headline feature
3. **Build Bridge service** (4-6 weeks) — Start with Redshift + Databricks. Skip Oracle/Tableau unless a paying customer requires them
4. **Do not convert to SaaS** — The market has enough cloud GIS tools. Local-first is defensible because it's hard to do well and the compliance market is growing

### What to tell customers

**Today**: "Your raw data never leaves your machine. AI analysis uses Google Gemini with your API key — only table schemas and sample values are sent, never full datasets. Address geocoding tries local matching first and falls back to Geocodio only when needed."

**After local LLM + Bridge**: "Spatia runs entirely on your machine. No cloud, no accounts, no data transmission. For enterprise data sources, deploy our Bridge agent in your VPC to pull data without it ever touching a third-party service."

---

## Key Code References

| Component | File | Relevance |
|-----------|------|-----------|
| Gemini API call | `src-tauri/crates/ai/src/client.rs:134-191` | All data sent to Google |
| Prompt construction (data leakage boundary) | `src-tauri/crates/ai/src/prompts.rs:9-32, 317-449` | Schema + samples + history |
| Geocodio API call | `src-tauri/crates/geocode/src/geocodio.rs:94-189` | Address strings sent to Geocodio |
| Local geocode pipeline | `src-tauri/crates/geocode/src/geocode.rs` | Cache → Overture → Geocodio fallback |
| DuckDB connection management | `src-tauri/crates/engine/src/db_manager.rs` | Single connection, no pooling |
| Analysis limits | `src-tauri/crates/engine/src/analysis.rs` | 1000-row GeoJSON, 5-step max |
| Tauri command surface | `src-tauri/src/lib.rs` | All 18 IPC entry points |
