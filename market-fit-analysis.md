# Spatia Market Fit Analysis
**Date:** March 13, 2026
**Version:** Based on current implemented feature set (MVP Sprint complete)

---

## Executive Summary

Spatia occupies a genuinely underserved position in the GIS software landscape: a local-first, AI-powered desktop tool that makes spatial data analysis accessible to analysts who know their data but do not know GIS. Its core strengths — local geocoding, natural language SQL generation against real spatial data, and offline operation — are meaningfully differentiated from ArcGIS Pro (too complex/expensive), Tableau (too limited on spatial), and Carto (enterprise-only, cloud-dependent). However, Carto's 2025 launch of AI Agents for natural language spatial queries narrows what was previously a unique Spatia advantage, adding urgency to reach market. The absence of data export, limited spatial analysis operators, and the CSV-only import constraint represent critical blockers that will prevent adoption even among users who are otherwise a perfect fit. This document maps those gaps against all three major competitors, ranks them by impact, and proposes a phased roadmap to market readiness.

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

Spatia's target quadrant is the lower-right: moderate-to-high spatial depth, genuinely low learning curve, AI-accelerated. No incumbent owns this space cleanly. Felt comes closest on ease of use but is cloud-only with weak analysis depth. Kepler.gl has depth but no AI and no geocoding.

### The Specific Gap Spatia Fills

**The ArcGIS Pro problem:** ArcGIS Pro is the gold standard for professional GIS but requires ESRI licensing ($1,500–$3,500+/year), weeks of onboarding, Windows-only installs for advanced features, and a toolbox paradigm that assumes GIS education. A city planner who needs to map 500 business locations and ask "which census tracts have the highest concentration?" cannot reasonably start with ArcGIS Pro.

**The Tableau problem:** Tableau's spatial support is genuinely limited. It can plot lat/lon points and join to pre-built geography files (states, zip codes), but it cannot geocode raw addresses locally, cannot execute spatial SQL, cannot buffer or intersect features, and produces no spatial outputs that other GIS tools can consume. A market analyst who builds Tableau dashboards hits a hard wall the moment spatial queries become non-trivial.

**The Carto problem:** Carto is Spatia's closest direct competitor and the most significant competitive threat. In 2025, Carto launched "Agentic GIS" — AI Agents that enable natural language spatial queries within map dashboards — directly overlapping Spatia's core UVP. Carto also offers low-code Workflows (100+ drag-and-drop spatial analysis tools), Builder (rich visualization powered by deck.gl), a Data Observatory with 12,000+ curated datasets, and native connections to BigQuery, Snowflake, and Redshift. However, Carto is enterprise-first: pricing starts at ~$199+/month (not publicly listed, requires sales contact), targets large teams (3–10+ editors, 15–50+ viewers per plan), requires cloud data warehouse infrastructure, and offers no offline or local-first capability. A solo analyst or small team with a CSV of addresses and no cloud data warehouse cannot use Carto cost-effectively.

**The gap Spatia fills:** An analyst with a CSV of addresses, a spatial question, and no GIS background should be able to get a map answer in under 10 minutes without cloud dependencies, subscription fees, or a GIS degree. Today, no tool makes this true. ArcGIS Pro demands GIS expertise. Tableau lacks spatial depth. Carto requires enterprise budgets and cloud infrastructure. Spatia is explicitly designed to make it true for individual analysts and small teams.

### Competitor Comparison Matrix

| Dimension | Spatia (current) | ArcGIS Pro | Tableau | QGIS | Kepler.gl | Felt | Carto |
|---|---|---|---|---|---|---|---|
| **Primary paradigm** | AI chat + map | Toolbox + GUI | BI + map | Desktop GIS | Visualization | Collaborative map | Agentic GIS + cloud analytics |
| **Target user** | Data analysts | GIS professionals | BI analysts | GIS/technical | Data engineers | Teams | Enterprise spatial teams |
| **Local/offline** | Yes (core) | Yes | No | Yes | No (browser) | No | No (cloud-native) |
| **Geocoding** | Local + API fallback | ESRI geocoder (paid) | None native | Plugin | None | Limited | API-based (Data Observatory) |
| **AI analysis** | Natural language SQL | Limited/Copilot add-on | Ask Data (basic) | None | None | None | AI Agents (NL in dashboards) |
| **Spatial SQL** | Via AI (DuckDB) | Python/Model Builder | None | Processing | None | None | PostGIS/BigQuery (via Workflows) |
| **Data import** | CSV only | CSV, Shapefile, GDB, GeoJSON, KML, Excel, PostGIS... | Excel, CSV, DB connections | 50+ formats | GeoJSON, CSV | GeoJSON, CSV | CSV, Shapefile, GeoJSON, BigQuery, Snowflake, Redshift |
| **Data export** | None currently | All formats | Excel, PDF, Tableau format | All formats | GeoJSON | GeoJSON, CSV | CSV, GeoJSON, API |
| **Pricing** | TBD (free/open?) | $1,500–$3,500+/year | $70–$840/user/year | Free | Free | Free tier + paid | ~$199+/month (enterprise, not public) |
| **Learning curve** | Low (by design) | Very high | Moderate | High | Moderate | Low | Moderate (Builder low, Workflows moderate) |
| **Deployment** | Desktop (Tauri) | Desktop (Windows) | Cloud + Desktop | Desktop | Browser | Browser | Cloud (SaaS or self-hosted enterprise) |

---

## 2. Feature Gap Analysis

### Data Import / Export

| Feature | Spatia | ArcGIS Pro | Tableau | Carto | Gap Classification |
|---|---|---|---|---|---|
| CSV import | Yes | Yes | Yes | Yes | — (Spatia competitive) |
| Shapefile import | No | Yes | No | Yes | Important Gap |
| GeoJSON import | No | Yes | Partial | Yes | Important Gap |
| KML/KMZ import | No | Yes | No | No | Nice-to-Have |
| Excel import | No | Yes | Yes | No | Important Gap |
| PostGIS connection | No | Yes | Yes | Yes (native) | Nice-to-Have |
| Cloud DW connection (BigQuery, Snowflake) | No | No | Yes | Yes (core architecture) | Important Gap (vs Carto) |
| Data export (CSV) | No | Yes | Yes | Yes | **Critical Gap** |
| Data export (Shapefile/GeoJSON) | No | Yes | No | Yes (GeoJSON) | **Critical Gap** |
| PDF / image export | No | Yes | Yes | Yes (PNG) | Important Gap |
| Chart image export | No | Yes | Yes | Yes | Important Gap |

**Summary:** The complete absence of any export path is the single most critical product gap. Users who derive a result cannot take it anywhere.

---

### Data Management and Preparation

| Feature | Spatia | ArcGIS Pro | Tableau | Carto | Gap Classification |
|---|---|---|---|---|---|
| AI data cleaning (multi-round) | Yes | No | No | No | **Spatia Advantage** |
| Data type detection | Yes (auto-schema) | Yes | Yes | Yes | Competitive |
| Table preview (50 rows) | Yes | Yes | Yes | Yes (Builder) | Competitive |
| Column filtering / sorting in UI | No | Yes | Yes | Yes | Important Gap |
| Search / filter rows | No | Yes | Yes | Yes | Important Gap |
| Data profiling / quality metrics | No | Yes | Tableau Prep | Yes (Data Observatory metadata) | Important Gap |
| Undo / redo | No | Yes | Yes | No | Nice-to-Have |
| Calculated fields | Via AI SQL | Yes | Yes | Yes (Workflows) | Partial (AI covers this) |
| Schema browsing | Via table preview | Full catalog | Full catalog | Yes (full catalog) | Important Gap |
| Saved queries / bookmarks | No | Yes | Yes | Yes (Workflows) | Nice-to-Have |
| Multi-table joins (user-driven) | Via AI SQL | Yes | Yes | Yes (Workflows drag-and-drop) | Partial (AI covers this) |
| Low-code data pipeline / ETL | No | ModelBuilder | Tableau Prep | Yes (Workflows — 100+ tools) | Important Gap (vs Carto) |

---

### Geocoding and Location Services

| Feature | Spatia | ArcGIS Pro | Tableau | Carto | Gap Classification |
|---|---|---|---|---|---|
| Address geocoding (local) | Yes (Overture) | ESRI locators | No | No (cloud API only) | **Spatia Advantage** |
| Address geocoding (API fallback) | Yes (Geocodio) | Yes (ESRI) | No | Yes (Data Observatory) | **Spatia Advantage** |
| Geocoding confidence / source metadata | Yes | Yes | N/A | Partial | **Spatia Advantage** |
| Persistent geocode cache | Yes (DuckDB) | No | N/A | No (cloud-based) | **Spatia Advantage** |
| Reverse geocoding | No | Yes | No | Yes | Nice-to-Have |
| POI search (Overture) | Yes | Yes | No | Yes (Data Observatory — 12K+ datasets) | Competitive (Carto has broader data) |
| IP / device location | No | No | No | Yes | Not applicable |
| Batch geocoding (CSV column) | Yes (auto-pipeline) | Yes | No | Yes (Workflows) | **Spatia Advantage** (offline) |

**Summary:** Geocoding is Spatia's strongest differentiator vs. ArcGIS Pro (no offline equivalent without expensive locators), Tableau (no native geocoding), and Carto (requires cloud infrastructure and enterprise pricing for equivalent capability). Spatia's local-first, offline geocoding with persistent caching is unique across all competitors. However, Carto's Data Observatory offers broader enrichment data (demographics, POIs, boundaries) that Spatia cannot match.

---

### Spatial Analysis and Operations

| Feature | Spatia | ArcGIS Pro | Tableau | Carto | Gap Classification |
|---|---|---|---|---|---|
| Natural language spatial SQL | Yes (AI) | No | No | Yes (AI Agents, 2025) | Competitive (Carto now overlaps) |
| Buffer / proximity | Via AI SQL (DuckDB spatial) | Yes (toolbox) | No | Yes (Workflows) | Partial (AI-mediated) |
| Intersect / union / difference | Via AI SQL | Yes (toolbox) | No | Yes (Workflows) | Partial (AI-mediated) |
| Point-in-polygon | Via AI SQL | Yes | No | Yes (Workflows) | Partial (AI-mediated) |
| Spatial joins | Via AI SQL | Yes | No | Yes (Workflows) | Partial (AI-mediated) |
| Nearest neighbor / network analysis | Via AI SQL (limited) | Yes | No | Yes (Workflows + isochrones) | Important Gap |
| Raster analysis | No | Yes | No | No | Nice-to-Have (out of scope) |
| Temporal / time-series analysis | Via AI SQL | Yes | Yes | Yes (Workflows) | Important Gap |
| Manual drawing / measurement tools | No | Yes | No | Yes (Builder) | Important Gap |
| Spatial statistics | Via AI SQL | Yes | No | Yes (Workflows) | Partial |
| Routing / network / isochrones | No | Yes (Network Analyst) | No | Yes (via LDS/APIs) | Important Gap (vs Carto) |
| Automated / scheduled analysis | No | ModelBuilder | No | Yes (Workflows — trigger via API, schedule) | Important Gap (vs Carto) |

**Note:** Spatia's AI SQL approach partially covers spatial analysis operations — DuckDB spatial extension supports most ST_ functions — but the coverage is entirely dependent on Gemini generating correct SQL. There is no direct user-driven geometry manipulation. Carto's Workflows provide a more reliable, deterministic alternative for spatial operations via drag-and-drop tools, though they require more spatial knowledge than Spatia's natural language approach.

---

### Map Visualization and Styling

| Feature | Spatia | ArcGIS Pro | Tableau | Carto | Gap Classification |
|---|---|---|---|---|---|
| Interactive map (pan/zoom) | Yes | Yes | Yes | Yes (Builder) | Competitive |
| Scatter/point layer (Deck.gl) | Yes | Yes | Yes | Yes (deck.gl native) | Competitive |
| Heatmap layer | Yes (implemented) | Yes | Limited | Yes | Competitive |
| Hexbin layer | Yes (implemented) | Yes | No | Yes (H3 hexagons) | Competitive (Carto uses H3) |
| Line/polygon layers (GeoJSON) | Yes | Yes | Yes | Yes | Competitive |
| Custom basemap selection | No (CartoDB dark only) | Yes | Yes | Yes (Carto basemaps + custom) | **Critical Gap** |
| Symbol/color customization | No (hardcoded) | Yes | Yes | Yes (Builder styling) | **Critical Gap** |
| Layer opacity / blending | No | Yes | Partial | Yes | Important Gap |
| Map legend | No | Yes | Yes | Yes (auto-generated) | **Critical Gap** |
| Map annotations / labels | No | Yes | Yes | Yes | Important Gap |
| Print-quality map export | No | Yes | Yes | Yes (PNG) | **Critical Gap** |
| Map scale bar / north arrow | No | Yes | Yes | Partial | Nice-to-Have |
| Multiple simultaneous layers | Partial (base + analysis + table) | Yes | Limited | Yes (unlimited layers) | Important Gap |
| Choropleth / graduated symbols | Via AI SQL only | Yes | Yes | Yes (Builder — core feature) | Important Gap |
| Billions of points rendering | No (1K limit) | Limited | Limited | Yes (deck.gl + cloud tiling) | Important Gap (vs Carto) |
| PMTiles vector tiles | Yes | No (ESRI tiles) | No | No (proprietary tiling) | **Spatia Advantage** |
| Offline map tiles | Yes | Limited | No | No | **Spatia Advantage** |

---

### Charts and Dashboards

| Feature | Spatia | ArcGIS Pro | Tableau | Carto | Gap Classification |
|---|---|---|---|---|---|
| Bar chart | Yes (Recharts) | Yes | Yes | Yes (Builder widgets) | Competitive |
| Pie chart | Yes (Recharts) | Yes | Yes | Yes | Competitive |
| Histogram | Yes (Recharts) | Yes | Yes | Yes | Competitive |
| Line / time-series chart | No | Yes | Yes | Yes | Important Gap |
| Scatter chart | No | Yes | Yes | Yes | Nice-to-Have |
| Chart export (PNG/PDF) | No | Yes | Yes | Yes | **Critical Gap** |
| Chart customization (colors, labels) | No | Yes | Yes | Yes (Builder) | Important Gap |
| Dashboard / multi-view layout | No | Yes | Yes | Yes (Builder — multi-widget dashboards) | Important Gap |
| Cross-filter (map + chart linked) | No | Yes | Yes | Yes (Builder — linked widgets) | Important Gap |
| Tabular results in chat | Yes (20 rows) | N/A | N/A | Via AI Agents | Competitive |
| Saved / shareable dashboards | No | Yes | Yes | Yes (Builder — shareable URLs, embeddable) | Important Gap (vs Carto) |
| Embeddable analytics | No | ArcGIS Online | Tableau Public | Yes (iframe embed, developer SDK) | Nice-to-Have |

---

### AI and ML Integration

| Feature | Spatia | ArcGIS Pro | Tableau | Carto | Gap Classification |
|---|---|---|---|---|---|
| Natural language query → map | Yes | Limited | Partial (Ask Data) | Yes (AI Agents in dashboards) | Competitive (Carto now overlaps) |
| AI data cleaning | Yes (multi-round) | No | Tableau Prep (rule-based) | No | **Spatia Advantage** |
| SQL auto-generation | Yes | No | No | Yes (AI Agents) | Competitive |
| Multi-turn conversation | Yes | No | Limited | Yes (AI Agents — conversational) | Competitive |
| Schema-aware AI context | Yes | No | No | Yes (AI Agents — warehouse-aware) | Competitive |
| AI error recovery (auto-retry) | Yes | No | No | Unknown | **Spatia Advantage** |
| AI-powered Workflows (MCP tools) | No | No | No | Yes (AI pipelines, MCP integration) | Important Gap (vs Carto) |
| AI provider flexibility | No (Gemini only) | N/A | N/A | Unknown (likely OpenAI) | Important Gap |
| Local / on-device AI | No | No | No | No | Nice-to-Have |
| ML model inference | No | Yes (ArcGIS ML tools) | Limited | Partial (via cloud DW) | Nice-to-Have |
| Predictive analytics | Via AI SQL | Yes | Yes | Via cloud DW | Partial |

**Note:** Carto's 2025 launch of AI Agents significantly narrows what was previously a clear Spatia advantage in natural language spatial analysis. Carto's AI Agents operate within dashboard contexts with access to cloud data warehouse schemas. However, Spatia retains advantages in: (1) AI data cleaning (no Carto equivalent), (2) fully offline AI analysis, and (3) lower barrier to entry (no enterprise infrastructure required).

---

### Collaboration and Sharing

| Feature | Spatia | ArcGIS Pro | Tableau | Carto | Gap Classification |
|---|---|---|---|---|---|
| Data export for handoff | No | Yes | Yes | Yes | **Critical Gap** |
| Shareable link | No | ArcGIS Online | Tableau Public/Server | Yes (Builder — public/private URLs) | Important Gap |
| Multi-user access | No | ESRI account | Yes | Yes (editor/viewer roles per plan) | Not in scope (desktop) |
| Version control / history | No | No | No | Partial (Workflow versioning) | Nice-to-Have |
| Embeddable maps | No | ArcGIS Online | Tableau Public | Yes (iframe + developer SDK) | Not in scope (desktop) |
| Comment / annotation layer | No | Yes | Yes | Partial | Nice-to-Have |
| API access to results | No | Yes | Yes | Yes (full REST API) | Nice-to-Have |

---

### Ease of Use and Learning Curve

| Dimension | Spatia | ArcGIS Pro | Tableau | Carto | Assessment |
|---|---|---|---|---|---|
| Time to first result (new user) | < 10 min (target) | Hours–days | 30–60 min | 30–60 min (requires DW setup) | **Spatia Advantage (potential)** |
| GIS knowledge required | Low | High | Low | Low–moderate | **Spatia Advantage** |
| Onboarding / empty state UX | Yes (implemented) | Yes | Yes | Yes (14-day trial + demos) | Competitive |
| In-app error guidance | Partial (API key banners) | Yes | Yes | Yes | Important Gap |
| Documentation / help | Minimal | Extensive | Extensive | Extensive (docs + academy) | **Critical Gap** |
| Setup complexity | Medium (PMTiles, env vars) | Low (installer) | Low (installer) | Medium (requires cloud DW account) | Important Gap |
| Settings UI | None (env vars only) | Full GUI | Full GUI | Full GUI (web-based) | **Critical Gap** |
| Infrastructure prerequisite | None (local files) | None | Tableau Cloud account | Cloud data warehouse (BigQuery/Snowflake/Redshift) | **Spatia Advantage** |

**Note:** Spatia's onboarding is actively harmed by requiring manual PMTiles setup and env-var configuration. This is not discoverable by a non-technical user. However, Carto also has a significant onboarding barrier: users must have or create a cloud data warehouse account before Carto can analyze their data. Spatia's local-file approach is simpler once the setup friction is resolved.

---

### Deployment and Pricing

| Dimension | Spatia | ArcGIS Pro | Tableau | Carto | Assessment |
|---|---|---|---|---|---|
| Offline capable | Yes (full) | Partial | No | No (cloud-native) | **Spatia Advantage** |
| Desktop install | Yes (Tauri) | Yes (Windows primary) | Yes | No (browser-based) | Competitive |
| macOS support | Yes | Limited | Yes | Yes (browser) | **Spatia Advantage** (native) |
| Linux support | Yes (Tauri) | No | Yes | Yes (browser) | **Spatia Advantage** (native) |
| Air-gapped operation | Yes | Partial | No | No | **Spatia Advantage** |
| Self-hosted option | Yes (inherent — desktop) | No | Tableau Server | Yes (Strategic tier only) | **Spatia Advantage** |
| Pricing | TBD | $1,500–$3,500+/year | $70–$840/user/year | ~$199+/month (enterprise, not public) | **Spatia Advantage (potential)** |
| License model | TBD | Subscription | Subscription | Subscription (usage units + seats) | TBD |
| IT deployment complexity | Medium | High | Medium | Medium-high (requires cloud DW) | Competitive |
| Data sovereignty | Full (local DuckDB) | Partial | No (cloud) | No (cloud DW dependent) | **Spatia Advantage** |

---

## 3. Unique Value Propositions

These are the areas where Spatia is genuinely ahead of every named competitor and where differentiated positioning should be built.

### UVP 1: Local-First AI Geocoding

No other tool in the target segment geocodes raw address data locally, offline, against real Overture Maps data with confidence scoring and a persistent cache. ArcGIS Pro requires ESRI locator licenses. Tableau does not geocode raw addresses at all. Carto offers geocoding but only via cloud APIs against its Data Observatory — requiring both an enterprise subscription and internet connectivity. Felt and Kepler require uploading pre-geocoded data. This is a genuine capability gap that solves a painful real-world problem: most analyst data starts as address strings, not lat/lon coordinates.

### UVP 2: Natural Language Spatial SQL Without a SQL Background

Spatia's AI chat generates and executes spatially-aware DuckDB SQL from plain English questions, with schema injection ensuring the AI knows what columns and tables actually exist. The user never writes SQL. ArcGIS Pro's Python/ModelBuilder path requires GIS training. Tableau's Ask Data is limited to BI aggregations and cannot perform true spatial operations (buffer, intersect, point-in-polygon).

**Carto competitive note:** Carto's AI Agents (launched 2025) now provide a similar natural language → spatial query experience within their Builder dashboards. This narrows what was previously a unique Spatia advantage. However, Spatia's approach differs in key ways: (1) Spatia works against local DuckDB data with no cloud infrastructure required, (2) Spatia's cleaning-geocoding-analysis pipeline is integrated end-to-end, and (3) Spatia's barrier to entry is dramatically lower (no enterprise contract, no data warehouse setup). The positioning shifts from "only tool with NL spatial queries" to "the only NL spatial query tool accessible to individual analysts and small teams."

### UVP 3: Fully Offline, Air-Gapped Operation

Spatia can run with zero network connectivity once set up. All data stays local: DuckDB database, PMTiles vector tiles, Overture geocoding tables. For healthcare analysts, government contractors, journalists working with sensitive data, or users in low-bandwidth environments, this is a hard requirement that cloud tools cannot meet. Carto, Tableau, and Felt are all cloud-dependent. This UVP becomes stronger as data sovereignty regulations tighten globally.

### UVP 4: AI-Powered Data Cleaning Before Geocoding

The multi-round Gemini cleaning pipeline normalizes address data before geocoding, increasing match rates without manual intervention. No competitor in the target segment — including Carto — combines AI cleaning, geocoding, and analysis in one automated pipeline. Carto's Workflows offer data transformation but not AI-driven data quality improvement.

### UVP 5: Zero Subscription, Zero ESRI Dependency

Spatia's stack has no runtime dependency on any commercial spatial data provider (ESRI, Mapbox, Google Maps). PMTiles are open, Overture is open, DuckDB is open, Tauri is open. The Gemini API key and optional Geocodio key are the only paid external dependencies, and both are user-supplied. By contrast, Carto requires an enterprise subscription (~$199+/month minimum) and a cloud data warehouse account (BigQuery, Snowflake, or Redshift) — adding both direct and infrastructure costs that are prohibitive for individual analysts and small teams.

---

## 4. Critical Feature Gaps (Priority Ranked)

### Gap 1: Data Export (Critical — Blocks All Real-World Use)

**What the gap is:** Spatia has no mechanism to export data. A user who runs a geocoding pipeline, derives analysis results, or generates charts cannot get the data out. No CSV download, no GeoJSON export, no Shapefile, no PDF, no PNG.

**Why it matters:** Every analyst workflow ends with sharing results. A tool that produces a map the user cannot share or a result set they cannot export into a report is a dead end. Even if everything else works perfectly, the inability to export makes Spatia unsuitable for any professional workflow. This is not a convenience feature — it is a fundamental requirement for the tool to have any value beyond exploration.

**How competitors handle it:** ArcGIS Pro exports to every major format. Tableau exports to Excel, PDF, and Tableau format. Carto exports CSV and GeoJSON from Builder and via API. Even Kepler.gl exports GeoJSON. This is table stakes.

**Suggested approach for Spatia:** Implement in order: (1) CSV export of any table from the FileList panel, (2) GeoJSON export of the current analysis_result view, (3) PNG export of the current map viewport. The first two are Rust/DuckDB operations. The third requires a MapLibre canvas capture.

---

### Gap 2: Settings UI and Configuration Discoverability (Critical — Blocks First-Time Users)

**What the gap is:** Spatia has no settings UI. API keys (Gemini, Geocodio), PMTiles file paths, and other configuration are all set via environment variables. There is no in-app way to configure the tool. PMTiles must be manually placed on disk with no in-app guidance on how to obtain or install them.

**Why it matters:** The target user — a market analyst or city planner — will not open a terminal to set environment variables. They will not know what a PMTiles file is or where to put it. The onboarding wall is insurmountable for the target persona without a UI-based configuration path. A user who cannot get past initial setup never experiences Spatia's genuine advantages.

**How competitors handle it:** Every competitor uses a GUI-based settings panel or guided onboarding wizard. Carto offers a 14-day free trial with demo datasets pre-loaded, eliminating the cold-start problem entirely.

**Suggested approach for Spatia:** A settings panel (accessible from the toolbar) should allow: entering API keys (stored securely via Tauri's secure storage), selecting PMTiles files via a file picker dialog, and testing configuration (verify API keys respond, verify PMTiles are valid). API keys should never be written to env files by the app — use Tauri's keystore or an app-local config file.

---

### Gap 3: Map Legend (Critical — Maps Without Legends Are Not Usable Artifacts)

**What the gap is:** Spatia renders map layers with hardcoded colors but displays no legend. A user viewing a scatter plot, heatmap, or hexbin layer cannot determine what they are looking at. There is no indication of what the color scale means, what the layer represents, or what data it shows.

**Why it matters:** A map without a legend is not a finished analytical artifact. It cannot be shared in a report, presented in a meeting, or used as evidence for a decision. This gap directly undercuts the core use case of "view results on map."

**How competitors handle it:** All GIS tools and BI tools provide automatic legend generation tied to the active layer's symbology. Carto Builder auto-generates legends for all layer types including choropleths and graduated symbols.

**Suggested approach for Spatia:** Auto-generate a legend panel within the map view that reflects: (1) the current Deck.gl layer type and its color encoding, (2) the data source name, (3) for quantitative scales, the min/max range. This can be a fixed-position overlay inside MapView rendered from the appStore's current layer state.

---

### Gap 4: Map Export / Print (Critical — Output That Can Be Shared)

**What the gap is:** There is no way to export the current map as an image, PDF, or printable layout.

**Why it matters:** "I made a map in Spatia" is only useful if the map can exit Spatia. Journalists writing data stories, planners presenting to councils, researchers publishing papers — all need a static map image. Without export, Spatia is a workflow tool with no deliverable.

**How competitors handle it:** ArcGIS Pro has full print layouts with north arrows and scale bars. Tableau exports PDF and PNG. Carto Builder exports map images as PNG. Kepler.gl exports PNG.

**Suggested approach for Spatia:** MapLibre GL's canvas can be captured as a PNG via `map.getCanvas().toDataURL()`. Wire this to a "Export Map" button. Initially ship without print layout framing (no north arrow, no scale bar) — just the current viewport as PNG.

---

### Gap 5: Custom Basemap Selection (Critical — One Dark Basemap Is Not Enough)

**What the gap is:** The only basemap is CartoDB dark matter. There is no way to select a light basemap, a satellite view, or a neutral base suitable for sharing with stakeholders.

**Why it matters:** CartoDB dark is appropriate for exploratory data visualization in developer contexts but is inappropriate for formal reports, presentations, or publication. A city planner presenting to a city council cannot present a dark-themed map. A real estate analyst including a map in a pitch deck needs a clean, light basemap.

**How competitors handle it:** All map-centric tools offer at least 3–5 basemap options. Felt's entire value proposition is attractive, contextual basemaps. Carto offers its own suite of basemaps plus custom basemap support — notably, Carto's dark basemap (CartoDB dark matter) is the same one Spatia currently uses as its only option.

**Suggested approach for Spatia:** Add a basemap selector to the map toolbar offering at minimum: CartoDB Dark, CartoDB Light (Positron), and OpenStreetMap. All three are free-to-use tile services. This is a small UI change with significant professional presentation impact. If the offline constraint is important for a given use case, the PMTiles can serve as an offline basemap option.

---

### Gap 6: GeoJSON and Shapefile Import (Important — Limits Data Sources Severely)

**What the gap is:** Spatia only ingests CSV files. The entire spatial data ecosystem is built on Shapefile, GeoJSON, KML, GeoPackage, and WFS. An analyst who has boundary data, administrative zones, or any spatial dataset from a government open data portal cannot load it into Spatia.

**Why it matters:** Real-world spatial analysis involves combining point data (addresses from CSV) with polygon data (census tracts, administrative boundaries, service areas). Without polygon ingestion, spatial joins and geographic aggregations are impossible even via AI SQL. The "which census tract" class of questions cannot be answered without boundary data.

**How competitors handle it:** ArcGIS Pro and QGIS handle 50+ spatial formats. Carto imports CSV, Shapefile, and GeoJSON, plus connects natively to BigQuery, Snowflake, and Redshift. Even Kepler.gl and Felt accept GeoJSON. CSV-only is a hard constraint that makes complex spatial analysis impossible.

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

**How competitors handle it:** DBeaver, pgAdmin, and QGIS DB Manager all provide direct SQL access. Carto Workflows allow users to build deterministic spatial analysis pipelines via drag-and-drop when AI-generated queries are insufficient. Even no-code tools like Retool expose SQL when needed.

**Suggested approach for Spatia:** A collapsible SQL editor panel in the ChatCard that shows the last AI-generated SQL and allows the user to edit and re-execute it. This does not need to be a full SQL IDE — just an editable text area with a Run button. Execution must still go through the existing safety validator.

---

### Gap 9: In-App Documentation and Help (Critical — Discovery and Trust)

**What the gap is:** Spatia has no in-app help, tooltips, documentation, or guided workflows. A new user who opens the app has the empty state onboarding (recently implemented) but no guidance on what Spatia can do, what kinds of questions to ask the AI, or how to set it up.

**Why it matters:** The target user is not a GIS practitioner. They do not have a mental model for what "spatial analysis" means. Without contextual guidance — example queries, capability descriptions, setup instructions — the app will feel opaque and users will abandon it before reaching the "aha" moment.

**How competitors handle it:** Tableau has extensive in-product help, sample workbooks, and a tooltip on every UI element. Carto provides extensive documentation, a learning academy, demo datasets, and contextual help within Builder. Felt has guided onboarding. QGIS has a built-in documentation browser.

**Suggested approach for Spatia:** (1) Tooltip labels on all UI controls (currently unlabeled icon buttons), (2) Example query chips in the ChatCard when no conversation is in progress (e.g., "Show me a heatmap of all points", "Which areas have the highest density?"), (3) A link to web documentation from the settings panel.

---

### Gap 10: Result Row Limit (Important — Analysis Completeness)

**What the gap is:** Analysis results are capped at 1,000 GeoJSON features and 20 tabular rows. For any dataset of meaningful size, this means the map and table show incomplete results without any indication that data has been truncated.

**Why it matters:** A user who asks "show me all 3,500 customers in the Pacific Northwest" sees 1,000 points on the map and may believe that is the full answer. Silent truncation without a visible indicator erodes trust in the tool's analytical accuracy. For tabular queries (aggregations, top-N) the 20-row limit means any query returning more than 20 rows produces an incomplete table.

**How competitors handle it:** ArcGIS Pro and QGIS render all features (within hardware limits) with explicit feedback on feature counts. Tableau uses extracts and streaming for large datasets. Carto Builder renders billions of data points via cloud-side tiling and deck.gl, making this a non-issue at any scale.

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

### Risk 7: Carto's Agentic GIS May Close the AI Analysis Gap for Enterprise Users

**Description:** Carto launched AI Agents in 2025, providing natural language spatial queries within Builder dashboards. This directly overlaps Spatia's core UVP of "natural language → spatial analysis." Carto has the advantage of enterprise distribution (existing customer base in transportation, retail, real estate, urban planning), cloud data warehouse integration (BigQuery, Snowflake, Redshift), 100+ Workflow tools for deterministic spatial operations, and a Data Observatory with 12,000+ curated datasets. If Carto successfully makes AI Agents accessible to mid-market teams (not just enterprise), Spatia's target users may see Carto as the safer choice.

**Probability:** Medium-High. Carto is actively investing in this direction and has the distribution advantage.

**Impact:** High. If Carto offers a comparable NL spatial query experience with enterprise backing, Spatia's differentiation narrows to three pillars: (1) offline/local-first operation, (2) AI data cleaning, and (3) zero-subscription accessibility. These are real but niche differentiators compared to Carto's breadth.

**What needs validation:** (1) Monitor Carto's AI Agent capabilities vs. Spatia's — can Carto's agents handle the same query types from raw CSV data without pre-existing cloud DW setup? (2) Track whether Carto introduces a lower-cost tier or free tier targeting individual analysts. (3) Validate whether Spatia's target personas (individual analysts, small teams, budget-constrained orgs) would even consider Carto given its enterprise pricing and infrastructure requirements.

**Mitigation:** Double down on Spatia's unique advantages that Carto structurally cannot replicate: offline operation, local-first data sovereignty, zero-infrastructure setup (once settings UI is built), and the integrated clean-geocode-analyze pipeline. Position explicitly against Carto: "Spatia is what Carto would be if it worked offline, cost nothing, and didn't require a cloud data warehouse."

---

### Key Assumptions Requiring Validation

1. **Target users will tolerate AI errors** as long as there is a clear path to correction. Current evidence is insufficient.
2. **Local geocoding quality is high enough** that the Geocodio fallback is rarely needed. Overture address coverage in non-US geographies is significantly lower.
3. **DuckDB spatial SQL coverage** via Gemini prompt engineering is sufficient for the "80% of spatial questions analysts actually ask" without requiring direct SQL access. This needs measurement.
4. **The offline constraint is a genuine differentiator** rather than an edge case. Validate how many target users are in environments where cloud tools are prohibited or unreliable.
5. **Analysts will invest in setup** (PMTiles, Overture extract) if the value proposition is clear. Currently unvalidated.
6. **Carto's enterprise pricing and DW requirement will remain a barrier** for Spatia's target persona. If Carto introduces a free or low-cost individual tier with CSV upload support, Spatia's positioning weakens significantly. Monitor Carto pricing changes.

---

## Summary Scorecard

| Category | Current State | vs ArcGIS Pro | vs Tableau | vs Carto | Phase to Address |
|---|---|---|---|---|---|
| Data Import | CSV only | Large gap | Moderate gap | Moderate gap | Phase 2 (GeoJSON/Shapefile) |
| Data Export | None | **Critical gap** | **Critical gap** | **Critical gap** | Phase 1 |
| Geocoding | Best-in-class | **Advantage** (local + free) | **Strong advantage** | **Advantage** (offline) | Maintain |
| AI Analysis | Strong | **Advantage** | **Advantage** | Competitive (Carto AI Agents) | Maintain + harden |
| Map Visualization | Functional | Large gap | Moderate gap | Large gap | Phase 1 |
| Map Export | None | **Critical gap** | **Critical gap** | **Critical gap** | Phase 1 |
| Charts / Dashboards | Basic (3 types) | Large gap | Large gap | Moderate gap | Phase 2 |
| Settings / Config | None (env vars) | **Critical gap** | **Critical gap** | **Critical gap** | Phase 1 |
| Spatial Operators | AI-mediated only | Large gap | **Advantage** | Moderate gap (Workflows) | Phase 2–3 |
| Collaboration | None | Moderate gap | Large gap | Large gap | Not in scope (desktop) |
| Learning Curve | Low (potential) | **Advantage** | Competitive | **Advantage** (no DW needed) | Phase 1 |
| Offline / Data Sovereignty | Best-in-class | **Advantage** | **Strong advantage** | **Strong advantage** | Maintain |
| Pricing | TBD (free/open?) | **Strong advantage** | **Advantage** | **Strong advantage** | Maintain |

### Competitive Position Summary

**vs ArcGIS Pro:** Spatia wins on accessibility, learning curve, pricing, and AI-first analysis. Loses heavily on spatial depth, data format support, visualization maturity, and enterprise features. Target users are ArcGIS Pro non-adopters, not switchers.

**vs Tableau:** Spatia wins on geocoding, spatial analysis, offline operation, and AI depth. Loses on dashboards, chart variety, collaboration, and ecosystem maturity. Target users are Tableau users who hit the spatial analysis wall.

**vs Carto:** This is the most nuanced competitive relationship. Carto is the closest direct competitor with overlapping AI capabilities (AI Agents) and spatial analysis depth (Workflows). Spatia wins on: offline/local-first operation, zero-subscription accessibility, AI data cleaning, and barrier to entry (no cloud DW required). Carto wins on: visualization at scale (billions of points), Workflows (deterministic spatial ops), Data Observatory (12K+ datasets), collaboration, and enterprise features. Target users are analysts who cannot or will not adopt Carto's enterprise pricing and cloud infrastructure requirements.

**Overall market readiness:** Pre-launch. The AI analysis and geocoding capabilities are genuinely differentiated and production-quality. However, four Critical-Blocking gaps (no export, no settings UI, no legend, no map export) mean the current build cannot be positioned as a complete product for any professional workflow. Phase 1 completion is a prerequisite for any public launch positioning. The emergence of Carto's AI Agents adds urgency — Spatia's window of differentiation on NL spatial queries is narrowing, making speed to market on Phase 1 items critical.
