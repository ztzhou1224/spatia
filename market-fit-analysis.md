# Spatia Market Fit Analysis
**Date:** March 13, 2026
**Version:** Based on current implemented feature set (MVP Sprint complete)

---

## Executive Summary

Spatia occupies a genuinely underserved position in the GIS software landscape: a local-first, AI-powered desktop tool that makes spatial data analysis accessible to analysts who know their data but do not know GIS. Its core strengths — local geocoding, natural language SQL generation against real spatial data, and offline operation — are meaningfully differentiated from every competitor in the target segment. However, the absence of data export, limited spatial analysis operators, and the CSV-only import constraint represent critical blockers that will prevent adoption even among users who are otherwise a perfect fit. This document maps those gaps, ranks them by impact, and proposes a phased roadmap to market readiness.

---

## 1. Positioning Analysis

### Where Spatia Sits in the Market Landscape

The GIS and spatial analytics tool market can be mapped along two axes:

- **Horizontal axis:** Technical depth (consumer-grade → professional GIS)
- **Vertical axis:** Ease of use (steep learning curve → self-service)

| Quadrant | Tools |
|---|---|
| High depth, high learning curve | ArcGIS Pro, QGIS |
| High depth, moderate curve | PostGIS + pgAdmin, FME |
| Moderate depth, low curve | Tableau (map view), Power BI (map visual) |
| Emerging: moderate depth, low curve + AI | Felt, Carto Builder, Kepler.gl, **Spatia** |

Spatia's target quadrant is the lower-right: moderate-to-high spatial depth, genuinely low learning curve, AI-accelerated. No incumbent owns this space cleanly. Felt comes closest on ease of use but is cloud-only with weak analysis depth. Kepler.gl has depth but no AI and no geocoding. Carto has both but targets enterprise.

### The Specific Gap Spatia Fills

**The ArcGIS Pro problem:** ArcGIS Pro is the gold standard for professional GIS but requires ESRI licensing ($1,500–$3,500+/year), weeks of onboarding, Windows-only installs for advanced features, and a toolbox paradigm that assumes GIS education. A city planner who needs to map 500 business locations and ask "which census tracts have the highest concentration?" cannot reasonably start with ArcGIS Pro.

**The Tableau problem:** Tableau's spatial support is genuinely limited. It can plot lat/lon points and join to pre-built geography files (states, zip codes), but it cannot geocode raw addresses locally, cannot execute spatial SQL, cannot buffer or intersect features, and produces no spatial outputs that other GIS tools can consume. A market analyst who builds Tableau dashboards hits a hard wall the moment spatial queries become non-trivial.

**The gap Spatia fills:** An analyst with a CSV of addresses, a spatial question, and no GIS background should be able to get a map answer in under 10 minutes without cloud dependencies, subscription fees, or a GIS degree. Today, no tool makes this true. Spatia is explicitly designed to make it true.

### Competitor Comparison Matrix

| Dimension | Spatia (current) | ArcGIS Pro | Tableau | QGIS | Kepler.gl | Felt | Carto |
|---|---|---|---|---|---|---|---|
| **Primary paradigm** | AI chat + map | Toolbox + GUI | BI + map | Desktop GIS | Visualization | Collaborative map | Cloud analytics |
| **Target user** | Data analysts | GIS professionals | BI analysts | GIS/technical | Data engineers | Teams | Enterprise |
| **Local/offline** | Yes (core) | Yes | No | Yes | No (browser) | No | No |
| **Geocoding** | Local + API fallback | ESRI geocoder (paid) | None native | Plugin | None | Limited | API-based |
| **AI analysis** | Natural language SQL | Limited/Copilot add-on | Ask Data (basic) | None | None | None | GPT-based (enterprise) |
| **Spatial SQL** | Via AI (DuckDB) | Python/Model Builder | None | Processing | None | None | PostGIS (technical) |
| **Data import** | CSV only | CSV, Shapefile, GDB, GeoJSON, KML, Excel, PostGIS... | Excel, CSV, DB connections | 50+ formats | GeoJSON, CSV | GeoJSON, CSV | CSV, PostGIS, BigQuery |
| **Data export** | None currently | All formats | Excel, PDF, Tableau format | All formats | GeoJSON | GeoJSON, CSV | CSV, GeoJSON |
| **Pricing** | TBD (free/open?) | $1,500–$3,500+/year | $70–$840/user/year | Free | Free | Free tier + paid | $199+/month |
| **Learning curve** | Low (by design) | Very high | Moderate | High | Moderate | Low | Moderate-high |
| **Deployment** | Desktop (Tauri) | Desktop (Windows) | Cloud + Desktop | Desktop | Browser | Browser | Cloud |

---

## 2. Feature Gap Analysis

### Data Import / Export

| Feature | Spatia | ArcGIS Pro | Tableau | Gap Classification |
|---|---|---|---|---|
| CSV import | Yes | Yes | Yes | — (Spatia competitive) |
| Shapefile import | No | Yes | No | Important Gap |
| GeoJSON import | No | Yes | Partial | Important Gap |
| KML/KMZ import | No | Yes | No | Nice-to-Have |
| Excel import | No | Yes | Yes | Important Gap |
| PostGIS connection | No | Yes | Yes | Nice-to-Have |
| Data export (CSV) | No | Yes | Yes | **Critical Gap** |
| Data export (Shapefile/GeoJSON) | No | Yes | No | **Critical Gap** |
| PDF / image export | No | Yes | Yes | Important Gap |
| Chart image export | No | Yes | Yes | Important Gap |

**Summary:** The complete absence of any export path is the single most critical product gap. Users who derive a result cannot take it anywhere.

---

### Data Management and Preparation

| Feature | Spatia | ArcGIS Pro | Tableau | Gap Classification |
|---|---|---|---|---|
| AI data cleaning (multi-round) | Yes | No | No | **Spatia Advantage** |
| Data type detection | Yes (auto-schema) | Yes | Yes | Competitive |
| Table preview (50 rows) | Yes | Yes | Yes | Competitive |
| Column filtering / sorting in UI | No | Yes | Yes | Important Gap |
| Search / filter rows | No | Yes | Yes | Important Gap |
| Data profiling / quality metrics | No | Yes | Tableau Prep | Important Gap |
| Undo / redo | No | Yes | Yes | Nice-to-Have |
| Calculated fields | Via AI SQL | Yes | Yes | Partial (AI covers this) |
| Schema browsing | Via table preview | Full catalog | Full catalog | Important Gap |
| Saved queries / bookmarks | No | Yes | Yes | Nice-to-Have |
| Multi-table joins (user-driven) | Via AI SQL | Yes | Yes | Partial (AI covers this) |

---

### Geocoding and Location Services

| Feature | Spatia | ArcGIS Pro | Tableau | Gap Classification |
|---|---|---|---|---|
| Address geocoding (local) | Yes (Overture) | ESRI locators | No | **Spatia Advantage** |
| Address geocoding (API fallback) | Yes (Geocodio) | Yes (ESRI) | No | **Spatia Advantage** |
| Geocoding confidence / source metadata | Yes | Yes | N/A | **Spatia Advantage** |
| Persistent geocode cache | Yes (DuckDB) | No | N/A | **Spatia Advantage** |
| Reverse geocoding | No | Yes | No | Nice-to-Have |
| POI search (Overture) | Yes | Yes | No | **Spatia Advantage** |
| IP / device location | No | No | No | Not applicable |
| Batch geocoding (CSV column) | Yes (auto-pipeline) | Yes | No | **Spatia Advantage** |

**Summary:** Geocoding is Spatia's strongest differentiator vs. both ArcGIS Pro (no offline equivalent without expensive locators) and Tableau (no native geocoding at all beyond country/state/zip matching).

---

### Spatial Analysis and Operations

| Feature | Spatia | ArcGIS Pro | Tableau | Gap Classification |
|---|---|---|---|---|
| Natural language spatial SQL | Yes (AI) | No | No | **Spatia Advantage** |
| Buffer / proximity | Via AI SQL (DuckDB spatial) | Yes (toolbox) | No | Partial (AI-mediated) |
| Intersect / union / difference | Via AI SQL | Yes (toolbox) | No | Partial (AI-mediated) |
| Point-in-polygon | Via AI SQL | Yes | No | Partial (AI-mediated) |
| Spatial joins | Via AI SQL | Yes | No | Partial (AI-mediated) |
| Nearest neighbor / network analysis | Via AI SQL (limited) | Yes | No | Important Gap |
| Raster analysis | No | Yes | No | Nice-to-Have (out of scope) |
| Temporal / time-series analysis | Via AI SQL | Yes | Yes | Important Gap |
| Manual drawing / measurement tools | No | Yes | No | Important Gap |
| Spatial statistics | Via AI SQL | Yes | No | Partial |
| Routing / network | No | Yes (Network Analyst) | No | Nice-to-Have |

**Note:** Spatia's AI SQL approach partially covers spatial analysis operations — DuckDB spatial extension supports most ST_ functions — but the coverage is entirely dependent on Gemini generating correct SQL. There is no direct user-driven geometry manipulation.

---

### Map Visualization and Styling

| Feature | Spatia | ArcGIS Pro | Tableau | Gap Classification |
|---|---|---|---|---|
| Interactive map (pan/zoom) | Yes | Yes | Yes | Competitive |
| Scatter/point layer (Deck.gl) | Yes | Yes | Yes | Competitive |
| Heatmap layer | Yes (implemented) | Yes | Limited | Competitive |
| Hexbin layer | Yes (implemented) | Yes | No | **Spatia Advantage** |
| Line/polygon layers (GeoJSON) | Yes | Yes | Yes | Competitive |
| Custom basemap selection | No (CartoDB dark only) | Yes | Yes | **Critical Gap** |
| Symbol/color customization | No (hardcoded) | Yes | Yes | **Critical Gap** |
| Layer opacity / blending | No | Yes | Partial | Important Gap |
| Map legend | No | Yes | Yes | **Critical Gap** |
| Map annotations / labels | No | Yes | Yes | Important Gap |
| Print-quality map export | No | Yes | Yes | **Critical Gap** |
| Map scale bar / north arrow | No | Yes | Yes | Nice-to-Have |
| Multiple simultaneous layers | Partial (base + analysis + table) | Yes | Limited | Important Gap |
| Choropleth / graduated symbols | Via AI SQL only | Yes | Yes | Important Gap |
| PMTiles vector tiles | Yes | No (ESRI tiles) | No | **Spatia Advantage** |
| Offline map tiles | Yes | Limited | No | **Spatia Advantage** |

---

### Charts and Dashboards

| Feature | Spatia | ArcGIS Pro | Tableau | Gap Classification |
|---|---|---|---|---|
| Bar chart | Yes (Recharts) | Yes | Yes | Competitive |
| Pie chart | Yes (Recharts) | Yes | Yes | Competitive |
| Histogram | Yes (Recharts) | Yes | Yes | Competitive |
| Line / time-series chart | No | Yes | Yes | Important Gap |
| Scatter chart | No | Yes | Yes | Nice-to-Have |
| Chart export (PNG/PDF) | No | Yes | Yes | **Critical Gap** |
| Chart customization (colors, labels) | No | Yes | Yes | Important Gap |
| Dashboard / multi-view layout | No | Yes | Yes | Important Gap |
| Cross-filter (map + chart linked) | No | Yes | Yes | Important Gap |
| Tabular results in chat | Yes (20 rows) | N/A | N/A | Competitive |
| Saved / shareable dashboards | No | Yes | Yes | Nice-to-Have |

---

### AI and ML Integration

| Feature | Spatia | ArcGIS Pro | Tableau | Gap Classification |
|---|---|---|---|---|
| Natural language query → map | Yes | Limited | Partial (Ask Data) | **Spatia Advantage** |
| AI data cleaning | Yes (multi-round) | No | Tableau Prep (rule-based) | **Spatia Advantage** |
| SQL auto-generation | Yes | No | No | **Spatia Advantage** |
| Multi-turn conversation | Yes | No | Limited | **Spatia Advantage** |
| Schema-aware AI context | Yes | No | No | **Spatia Advantage** |
| AI error recovery (auto-retry) | Yes | No | No | **Spatia Advantage** |
| AI provider flexibility | No (Gemini only) | N/A | N/A | Important Gap |
| Local / on-device AI | No | No | No | Nice-to-Have |
| ML model inference | No | Yes (ArcGIS ML tools) | Limited | Nice-to-Have |
| Predictive analytics | Via AI SQL | Yes | Yes | Partial |

---

### Collaboration and Sharing

| Feature | Spatia | ArcGIS Pro | Tableau | Gap Classification |
|---|---|---|---|---|
| Data export for handoff | No | Yes | Yes | **Critical Gap** |
| Shareable link | No | ArcGIS Online | Tableau Public/Server | Important Gap |
| Multi-user access | No | ESRI account | Yes | Not in scope (desktop) |
| Version control / history | No | No | No | Nice-to-Have |
| Embeddable maps | No | ArcGIS Online | Tableau Public | Not in scope (desktop) |
| Comment / annotation layer | No | Yes | Yes | Nice-to-Have |

---

### Ease of Use and Learning Curve

| Dimension | Spatia | ArcGIS Pro | Tableau | Assessment |
|---|---|---|---|---|
| Time to first result (new user) | < 10 min (target) | Hours–days | 30–60 min | **Spatia Advantage (potential)** |
| GIS knowledge required | Low | High | Low | **Spatia Advantage** |
| Onboarding / empty state UX | Yes (implemented) | Yes | Yes | Competitive |
| In-app error guidance | Partial (API key banners) | Yes | Yes | Important Gap |
| Documentation / help | Minimal | Extensive | Extensive | **Critical Gap** |
| Setup complexity | Medium (PMTiles, env vars) | Low (installer) | Low (installer) | Important Gap |
| Settings UI | None (env vars only) | Full GUI | Full GUI | **Critical Gap** |

**Note:** Spatia's onboarding is actively harmed by requiring manual PMTiles setup and env-var configuration. This is not discoverable by a non-technical user.

---

### Deployment and Pricing

| Dimension | Spatia | ArcGIS Pro | Tableau | Assessment |
|---|---|---|---|---|
| Offline capable | Yes (full) | Partial | No | **Spatia Advantage** |
| Desktop install | Yes (Tauri) | Yes (Windows primary) | Yes | Competitive |
| macOS support | Yes | Limited | Yes | **Spatia Advantage** |
| Linux support | Yes (Tauri) | No | Yes | **Spatia Advantage** |
| Air-gapped operation | Yes | Partial | No | **Spatia Advantage** |
| Pricing | TBD | $1,500–$3,500+/year | $70–$840/user/year | **Spatia Advantage (potential)** |
| License model | TBD | Subscription | Subscription | TBD |
| IT deployment complexity | Medium | High | Medium | Competitive |

---

## 3. Unique Value Propositions

These are the areas where Spatia is genuinely ahead of every named competitor and where differentiated positioning should be built.

### UVP 1: Local-First AI Geocoding

No other tool in the target segment geocodes raw address data locally, offline, against real Overture Maps data with confidence scoring and a persistent cache. ArcGIS Pro requires ESRI locator licenses. Tableau does not geocode raw addresses at all. Felt and Kepler require uploading pre-geocoded data. This is a genuine capability gap that solves a painful real-world problem: most analyst data starts as address strings, not lat/lon coordinates.

### UVP 2: Natural Language Spatial SQL Without a SQL Background

Spatia's AI chat generates and executes spatially-aware DuckDB SQL from plain English questions, with schema injection ensuring the AI knows what columns and tables actually exist. The user never writes SQL. ArcGIS Pro's Python/ModelBuilder path requires GIS training. Tableau's Ask Data is limited to BI aggregations and cannot perform true spatial operations (buffer, intersect, point-in-polygon). This lowers the barrier to spatial analysis from "requires GIS education" to "can type a question."

### UVP 3: Fully Offline, Air-Gapped Operation

Spatia can run with zero network connectivity once set up. All data stays local: DuckDB database, PMTiles vector tiles, Overture geocoding tables. For healthcare analysts, government contractors, journalists working with sensitive data, or users in low-bandwidth environments, this is a hard requirement that cloud tools cannot meet.

### UVP 4: AI-Powered Data Cleaning Before Geocoding

The multi-round Gemini cleaning pipeline normalizes address data before geocoding, increasing match rates without manual intervention. No competitor in the target segment combines cleaning, geocoding, and analysis in one pipeline.

### UVP 5: Zero Subscription, Zero ESRI Dependency

Spatia's stack has no runtime dependency on any commercial spatial data provider (ESRI, Mapbox, Google Maps). PMTiles are open, Overture is open, DuckDB is open, Tauri is open. The Gemini API key and optional Geocodio key are the only paid external dependencies, and both are user-supplied.

---

## 4. Critical Feature Gaps (Priority Ranked)

### Gap 1: Data Export (Critical — Blocks All Real-World Use)

**What the gap is:** Spatia has no mechanism to export data. A user who runs a geocoding pipeline, derives analysis results, or generates charts cannot get the data out. No CSV download, no GeoJSON export, no Shapefile, no PDF, no PNG.

**Why it matters:** Every analyst workflow ends with sharing results. A tool that produces a map the user cannot share or a result set they cannot export into a report is a dead end. Even if everything else works perfectly, the inability to export makes Spatia unsuitable for any professional workflow. This is not a convenience feature — it is a fundamental requirement for the tool to have any value beyond exploration.

**How competitors handle it:** ArcGIS Pro exports to every major format. Tableau exports to Excel, PDF, and Tableau format. Even Kepler.gl exports GeoJSON. This is table stakes.

**Suggested approach for Spatia:** Implement in order: (1) CSV export of any table from the FileList panel, (2) GeoJSON export of the current analysis_result view, (3) PNG export of the current map viewport. The first two are Rust/DuckDB operations. The third requires a MapLibre canvas capture.

---

### Gap 2: Settings UI and Configuration Discoverability (Critical — Blocks First-Time Users)

**What the gap is:** Spatia has no settings UI. API keys (Gemini, Geocodio), PMTiles file paths, and other configuration are all set via environment variables. There is no in-app way to configure the tool. PMTiles must be manually placed on disk with no in-app guidance on how to obtain or install them.

**Why it matters:** The target user — a market analyst or city planner — will not open a terminal to set environment variables. They will not know what a PMTiles file is or where to put it. The onboarding wall is insurmountable for the target persona without a UI-based configuration path. A user who cannot get past initial setup never experiences Spatia's genuine advantages.

**How competitors handle it:** Every competitor uses a GUI-based settings panel or guided onboarding wizard.

**Suggested approach for Spatia:** A settings panel (accessible from the toolbar) should allow: entering API keys (stored securely via Tauri's secure storage), selecting PMTiles files via a file picker dialog, and testing configuration (verify API keys respond, verify PMTiles are valid). API keys should never be written to env files by the app — use Tauri's keystore or an app-local config file.

---

### Gap 3: Map Legend (Critical — Maps Without Legends Are Not Usable Artifacts)

**What the gap is:** Spatia renders map layers with hardcoded colors but displays no legend. A user viewing a scatter plot, heatmap, or hexbin layer cannot determine what they are looking at. There is no indication of what the color scale means, what the layer represents, or what data it shows.

**Why it matters:** A map without a legend is not a finished analytical artifact. It cannot be shared in a report, presented in a meeting, or used as evidence for a decision. This gap directly undercuts the core use case of "view results on map."

**How competitors handle it:** All GIS tools and BI tools provide automatic legend generation tied to the active layer's symbology.

**Suggested approach for Spatia:** Auto-generate a legend panel within the map view that reflects: (1) the current Deck.gl layer type and its color encoding, (2) the data source name, (3) for quantitative scales, the min/max range. This can be a fixed-position overlay inside MapView rendered from the appStore's current layer state.

---

### Gap 4: Map Export / Print (Critical — Output That Can Be Shared)

**What the gap is:** There is no way to export the current map as an image, PDF, or printable layout.

**Why it matters:** "I made a map in Spatia" is only useful if the map can exit Spatia. Journalists writing data stories, planners presenting to councils, researchers publishing papers — all need a static map image. Without export, Spatia is a workflow tool with no deliverable.

**How competitors handle it:** ArcGIS Pro has full print layouts with north arrows and scale bars. Tableau exports PDF and PNG. Kepler.gl exports PNG.

**Suggested approach for Spatia:** MapLibre GL's canvas can be captured as a PNG via `map.getCanvas().toDataURL()`. Wire this to a "Export Map" button. Initially ship without print layout framing (no north arrow, no scale bar) — just the current viewport as PNG.

---

### Gap 5: Custom Basemap Selection (Critical — One Dark Basemap Is Not Enough)

**What the gap is:** The only basemap is CartoDB dark matter. There is no way to select a light basemap, a satellite view, or a neutral base suitable for sharing with stakeholders.

**Why it matters:** CartoDB dark is appropriate for exploratory data visualization in developer contexts but is inappropriate for formal reports, presentations, or publication. A city planner presenting to a city council cannot present a dark-themed map. A real estate analyst including a map in a pitch deck needs a clean, light basemap.

**How competitors handle it:** All map-centric tools offer at least 3–5 basemap options. Felt's entire value proposition is attractive, contextual basemaps.

**Suggested approach for Spatia:** Add a basemap selector to the map toolbar offering at minimum: CartoDB Dark, CartoDB Light (Positron), and OpenStreetMap. All three are free-to-use tile services. This is a small UI change with significant professional presentation impact. If the offline constraint is important for a given use case, the PMTiles can serve as an offline basemap option.

---

### Gap 6: GeoJSON and Shapefile Import (Important — Limits Data Sources Severely)

**What the gap is:** Spatia only ingests CSV files. The entire spatial data ecosystem is built on Shapefile, GeoJSON, KML, GeoPackage, and WFS. An analyst who has boundary data, administrative zones, or any spatial dataset from a government open data portal cannot load it into Spatia.

**Why it matters:** Real-world spatial analysis involves combining point data (addresses from CSV) with polygon data (census tracts, administrative boundaries, service areas). Without polygon ingestion, spatial joins and geographic aggregations are impossible even via AI SQL. The "which census tract" class of questions cannot be answered without boundary data.

**How competitors handle it:** ArcGIS Pro and QGIS handle 50+ spatial formats. Even Kepler.gl and Felt accept GeoJSON. CSV-only is a hard constraint that makes complex spatial analysis impossible.

**Suggested approach for Spatia:** DuckDB's spatial extension supports reading GeoJSON natively via `ST_Read`. Shapefile support requires the `spatial` extension's GDAL bindings (available in DuckDB 1.4.4). Extending the ingest pipeline to accept `.geojson` and `.shp` files is the highest-leverage import addition.

---

### Gap 7: Column Filtering and Sorting in Table Preview (Important — Basic Data Literacy)

**What the gap is:** The table preview shows 50 rows with no ability to filter, sort, or search. Users cannot verify data quality, check for outliers, or validate geocoding results without scrolling through a static grid.

**Why it matters:** Data exploration before analysis is a fundamental workflow step. A user who geocodes 500 addresses needs to quickly scan for failed matches or low-confidence results. The current 50-row static preview is insufficient for this purpose.

**How competitors handle it:** Every BI tool has column-level sort and filter. Even simple tools like Google Sheets have these.

**Suggested approach for Spatia:** Add column-level sort (toggle ascending/descending by column header click) and a row count indicator. Full filter is a phase 2 addition. The underlying DuckDB query can be extended with `ORDER BY` and `LIMIT/OFFSET` for pagination.

---

### Gap 8: Direct SQL Access (Important — Power User Path)

**What the gap is:** There is no way to write or execute SQL directly. All SQL is AI-generated. Users who know SQL cannot bypass the AI to run precise queries, debug AI-generated SQL, or perform operations the AI consistently gets wrong.

**Why it matters:** The target personas (market analysts, academic researchers, journalists) include SQL-literate users who will hit the AI's limits quickly. When the AI generates incorrect SQL, the user has no recourse but to re-prompt. A SQL console provides a direct escape hatch and builds user trust by making the AI's work transparent.

**How competitors handle it:** DBeaver, pgAdmin, and QGIS DB Manager all provide direct SQL access. Even no-code tools like Retool expose SQL when needed.

**Suggested approach for Spatia:** A collapsible SQL editor panel in the ChatCard that shows the last AI-generated SQL and allows the user to edit and re-execute it. This does not need to be a full SQL IDE — just an editable text area with a Run button. Execution must still go through the existing safety validator.

---

### Gap 9: In-App Documentation and Help (Critical — Discovery and Trust)

**What the gap is:** Spatia has no in-app help, tooltips, documentation, or guided workflows. A new user who opens the app has the empty state onboarding (recently implemented) but no guidance on what Spatia can do, what kinds of questions to ask the AI, or how to set it up.

**Why it matters:** The target user is not a GIS practitioner. They do not have a mental model for what "spatial analysis" means. Without contextual guidance — example queries, capability descriptions, setup instructions — the app will feel opaque and users will abandon it before reaching the "aha" moment.

**How competitors handle it:** Tableau has extensive in-product help, sample workbooks, and a tooltip on every UI element. Felt has guided onboarding. QGIS has a built-in documentation browser.

**Suggested approach for Spatia:** (1) Tooltip labels on all UI controls (currently unlabeled icon buttons), (2) Example query chips in the ChatCard when no conversation is in progress (e.g., "Show me a heatmap of all points", "Which areas have the highest density?"), (3) A link to web documentation from the settings panel.

---

### Gap 10: Result Row Limit (Important — Analysis Completeness)

**What the gap is:** Analysis results are capped at 1,000 GeoJSON features and 20 tabular rows. For any dataset of meaningful size, this means the map and table show incomplete results without any indication that data has been truncated.

**Why it matters:** A user who asks "show me all 3,500 customers in the Pacific Northwest" sees 1,000 points on the map and may believe that is the full answer. Silent truncation without a visible indicator erodes trust in the tool's analytical accuracy. For tabular queries (aggregations, top-N) the 20-row limit means any query returning more than 20 rows produces an incomplete table.

**How competitors handle it:** ArcGIS Pro and QGIS render all features (within hardware limits) with explicit feedback on feature counts. Tableau uses extracts and streaming for large datasets.

**Suggested approach for Spatia:** (1) Show an explicit count badge on the map layer indicating "showing X of Y features" when truncation occurs, (2) Increase the tabular result limit to 100 rows with pagination, (3) Add a "Download full result as CSV" link when truncation is active. The underlying DuckDB result can return the full count with a single `COUNT(*)` query before truncation.

---

## 5. Recommended Roadmap

### Phase 1: Table Stakes (Must Have Before Any Meaningful Launch)

These are capabilities whose absence will cause every target user to immediately identify Spatia as incomplete or unfinished. Without all of Phase 1, Spatia cannot be positioned as a production-ready tool.

| Priority | Feature | Rationale |
|---|---|---|
| P1.1 | **CSV export** of any loaded table | Cannot take data anywhere without this |
| P1.2 | **GeoJSON export** of analysis_result | Cannot share spatial outputs |
| P1.3 | **Map PNG export** (current viewport) | Cannot include map in any deliverable |
| P1.4 | **Settings UI** (API keys + PMTiles path via file picker) | Target users cannot configure env vars |
| P1.5 | **Map legend** (auto-generated from active layer) | Maps without legends are not interpretable |
| P1.6 | **Basemap selector** (dark + light + OSM minimum) | Dark-only maps are not presentation-ready |
| P1.7 | **Truncation indicator** on map and table results | Silent data loss destroys analytical trust |
| P1.8 | **Tooltip labels** on all UI controls | Basic discoverability requirement |

---

### Phase 2: Competitive Parity (Needed to Compete)

These features bring Spatia to a level where direct comparison against Felt, Kepler.gl, and lower-tier ArcGIS users is favorable.

| Priority | Feature | Rationale |
|---|---|---|
| P2.1 | **GeoJSON/Shapefile import** | Required for any real spatial analysis (joins, polygon overlays) |
| P2.2 | **Column sort and filter** in table preview | Basic data literacy feature every competitor has |
| P2.3 | **Editable SQL panel** (show last AI SQL, allow edit + re-run) | Power user escape hatch, builds AI transparency |
| P2.4 | **Chart export** (PNG for bar/pie/histogram) | Reports require chart images |
| P2.5 | **Map annotation** (static text labels on features) | Contextualizing map results |
| P2.6 | **Example query suggestions** in empty chat | Reduces first-use friction dramatically |
| P2.7 | **Increased result limits** (5,000 features, 100 table rows) with pagination | Completeness for real datasets |
| P2.8 | **Line and time-series chart** type | Time-based analysis is common in target personas |

---

### Phase 3: Differentiation (What Makes Users Choose Spatia Over Alternatives)

These features capitalize on Spatia's unique architecture — local-first, AI-native, DuckDB-powered — to create capabilities no competitor can easily replicate.

| Priority | Feature | Rationale |
|---|---|---|
| P3.1 | **Spatial analysis wizard** (buffer, intersect, point-in-polygon as guided UI flows) | Makes spatial ops accessible without AI or SQL knowledge |
| P3.2 | **Multi-layer map** with user-controlled visibility and ordering | Professional GIS workflow requirement |
| P3.3 | **Choropleth / graduated symbol** rendering (when polygon data + numeric column present) | Most impactful map visualization type for policy/analysis |
| P3.4 | **Saved analysis bookmarks** (name + re-run a query) | Reusable analysis for recurring workflows |
| P3.5 | **Cross-filter** (click map feature to filter chart, click chart bar to highlight map) | Linked views are the signature capability of BI tools; doing it spatially is unique |
| P3.6 | **AI model configurability** (add OpenAI/Anthropic key as alternative to Gemini) | Reduces single-provider dependency risk |
| P3.7 | **Overture data browser** (explore nearby POIs, load additional Overture themes) | Turns Spatia into a self-contained spatial data source, not just an analysis tool |
| P3.8 | **Temporal playback** (animate points over a time column) | Differentiating visualization for logistics, mobility, event data |

---

## 6. Risk Assessment

### Risk 1: The Setup Friction Wall May Be Insurmountable for Target Personas

**Description:** Spatia currently requires manual PMTiles installation (requiring knowledge of what PMTiles are, where to obtain them, and where to place them) and environment variable configuration for API keys. The target user — a non-technical market analyst or city planner — cannot be expected to complete this setup independently.

**Probability:** High. This is the current state.

**Impact:** Critical. An analyst who cannot get past setup never evaluates the product's value.

**What needs validation:** Test setup completion rate with 5 non-GIS users given only a README and no live support. If fewer than 80% complete setup independently, the setup experience requires fundamental redesign before launch.

**Mitigation:** Bundling a default PMTiles set covering major metro areas (or using a free online tile service as a default), and replacing env var config with a first-run setup wizard, would eliminate this blocker.

---

### Risk 2: Single-Provider AI Dependency (Gemini Only)

**Description:** Spatia is hard-coupled to Gemini. If Google changes pricing, API behavior, rate limits, or deprecates the API version in use, Spatia's core AI functionality breaks entirely with no fallback.

**Probability:** Medium. Google has historically changed API pricing and terms with limited notice.

**Impact:** High. AI analysis and AI cleaning both fail without Gemini. The product loses two of its three core differentiators.

**What needs validation:** Determine whether the Gemini API version in use is stable or in preview/beta. Preview APIs have shorter deprecation windows.

**Mitigation:** Abstract the AI client layer (already partially done in the `spatia_ai` crate) to support pluggable providers. OpenAI and Anthropic are the obvious alternatives. This is a Phase 3 item but should not be deprioritized below Phase 3 given the concentration risk.

---

### Risk 3: The "AI Did It Wrong" Dead End

**Description:** When Gemini generates incorrect SQL, the current UX is a multi-turn conversation loop where the user re-phrases the question hoping the AI gets it right next time. There is no escape hatch (direct SQL editing), no explanation of why the query failed, and no indication of what the AI tried to do.

**Probability:** High. AI SQL generation fails on ambiguous or complex spatial queries. This is an inherent property of LLM-based code generation.

**Impact:** Medium-High. A user who hits two or three bad AI responses will conclude that Spatia does not work, regardless of whether a corrected query would succeed. This is a trust and retention issue.

**What needs validation:** Log the actual SQL failure rate per session type (simple point query vs. spatial join vs. aggregation). If the failure rate for spatial joins exceeds 20%, the AI loop requires significant prompt engineering improvements before launch.

**Mitigation:** SQL editing panel (Phase 2), explicit error explanations in the AI response, and better prompt engineering for spatial operations.

---

### Risk 4: PMTiles Delivery Problem at Scale

**Description:** Spatia requires users to obtain and install PMTiles files locally. There is no in-app mechanism to download, update, or manage these files. For the Overture geocoding pipeline to work, users must also run the `overture_extract` CLI command, which downloads Overture parquet data from S3.

**Probability:** High (for non-technical users). This is the current architecture.

**Impact:** High. Without tiles and Overture data, the map is blank and geocoding falls back entirely to Geocodio (paid API). Two of Spatia's UVPs (offline operation and local geocoding) disappear.

**What needs validation:** Whether a curated, pre-built bundle of PMTiles + Overture extract for top 20 US metro areas can be shipped with the installer or made available as a one-click download within the app.

---

### Risk 5: Market Education Burden

**Description:** Spatia sits in a category that does not yet exist in users' mental models. Users know "I need to make a map" (ArcGIS, Google Maps) or "I need to analyze data" (Excel, Tableau). They do not know "I need to geocode my data, run spatial analysis via natural language, and explore results on a map." Spatia must simultaneously create the category and win it.

**Probability:** Medium. This is a positioning and marketing challenge, not a product failure.

**Impact:** Medium. Requires sustained positioning, clear use-case marketing ("for analysts who have address data and spatial questions"), and rapid time-to-value once the setup friction issues are resolved.

**What needs validation:** Which entry-point use cases create the fastest "aha" moment? Hypothesis: "Upload a CSV of customer addresses and ask 'show me where my customers are concentrated'" is the fastest path to perceived value. This should be the onboarding flow, not an open-ended blank canvas.

---

### Risk 6: The 1,000-Feature GeoJSON Limit Undermines Analytical Credibility

**Description:** Silent truncation of results at 1,000 features means any analysis on a dataset of typical business size (1,000–100,000 rows) will show incomplete map results. A user analyzing 10,000 store locations sees 1,000 points and may draw incorrect conclusions.

**Probability:** High. Any real-world dataset exceeds 1,000 features.

**Impact:** High for analytical credibility. If a user discovers that their map is showing 10% of their data, they will not trust any other result Spatia produces.

**What needs validation:** Deck.gl performance at 10,000 and 50,000 points with ScatterplotLayer. DuckDB GeoJSON serialization time at the same scales. Expected performance budget is sub-2-second render for 50,000 points in a ScatterplotLayer (Deck.gl handles this comfortably on modern hardware).

---

### Key Assumptions Requiring Validation

1. **Target users will tolerate AI errors** as long as there is a clear path to correction. Current evidence is insufficient.
2. **Local geocoding quality is high enough** that the Geocodio fallback is rarely needed. Overture address coverage in non-US geographies is significantly lower.
3. **DuckDB spatial SQL coverage** via Gemini prompt engineering is sufficient for the "80% of spatial questions analysts actually ask" without requiring direct SQL access. This needs measurement.
4. **The offline constraint is a genuine differentiator** rather than an edge case. Validate how many target users are in environments where cloud tools are prohibited or unreliable.
5. **Analysts will invest in setup** (PMTiles, Overture extract) if the value proposition is clear. Currently unvalidated.

---

## Summary Scorecard

| Category | Current State | Gap Severity | Phase to Address |
|---|---|---|---|
| Data Import | CSV only | Critical | Phase 2 (GeoJSON/Shapefile) |
| Data Export | None | **Critical — Blocking** | Phase 1 |
| Geocoding | Best-in-class | Advantage | Maintain |
| AI Analysis | Best-in-class | Advantage | Maintain + harden |
| Map Visualization | Functional | Critical gaps (legend, basemap) | Phase 1 |
| Map Export | None | **Critical — Blocking** | Phase 1 |
| Charts | Basic (3 types) | Adequate for MVP | Phase 2 |
| Settings / Config | None (env vars) | **Critical — Blocking** | Phase 1 |
| Spatial Operators | AI-mediated only | Important | Phase 2–3 |
| Sharing / Export | None | **Critical — Blocking** | Phase 1 |
| Learning Curve | Low (potential) | Partially blocked by setup | Phase 1 |
| Offline Operation | Best-in-class | Advantage | Maintain |

**Overall market readiness:** Pre-launch. The AI analysis and geocoding capabilities are genuinely differentiated and production-quality. However, four Critical-Blocking gaps (no export, no settings UI, no legend, no map export) mean the current build cannot be positioned as a complete product for any professional workflow. Phase 1 completion is a prerequisite for any public launch positioning.
