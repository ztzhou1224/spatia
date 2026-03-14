---
name: feedback_patterns
description: Recurring domain feedback themes across Spatia review sessions
type: feedback
---

## ST_Distance degree vs meter confusion is the #1 pitfall

DuckDB spatial's ST_Distance on WGS84 lat/lon geometry returns degrees, not meters. Any distance-based query (buffer, proximity, "within X miles") will produce wrong results unless the AI or the engine explicitly handles unit conversion. This has come up as the most critical spatial correctness issue.

**Why:** Real users will not notice degree-based distances in the UI unless results look obviously wrong. A "within 1 mile" query silently returning "within ~0.014 degrees" is a correctness failure that erodes trust.
**How to apply:** Flag any analysis prompt or test case involving distance and verify unit handling. Recommend the prompt explicitly instruct the model to use ST_DWithin with degree conversion or project to a local CRS.

## Confidence score visibility is critical for geocoding UX

Users need to see and override low-confidence geocode matches. A threshold of 0.6 is often wrong in practice. Without per-row confidence display and manual override, the geocoding pipeline is not production-ready for government/planning use cases.

**Why:** Government data analysts are accountable for data quality. Silently accepting a bad geocode match then presenting it to a city council is a real career risk for users.
**How to apply:** When evaluating geocoding UX, always ask whether confidence scores are surfaced and whether users can flag/correct low-confidence results.

## Export to image/PDF is a baseline expectation

Every GIS practitioner persona needs to get a map out of the app and into a report, presentation, or email. If Spatia cannot export a map view, it is not a complete workflow tool regardless of analysis quality.

**How to apply:** Treat map export as a blocking gap when evaluating workflow completeness for any persona.
