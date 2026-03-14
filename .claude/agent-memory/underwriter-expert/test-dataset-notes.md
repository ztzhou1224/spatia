# Test Dataset: commercial_property_portfolio.csv

**Path**: `/home/user/spatia/data/commercial_property_portfolio.csv`
**Rows**: 75 | **Columns**: 33 | **Created**: 2026-03-14

## Field Inventory

| Column | Type | Notes |
|---|---|---|
| policy_number | string | Format CPL-YYYY-NNNNN; multi-location policies share same number |
| insured_name | string | Legal entity name |
| location_number | integer | 1-N per policy for multi-location |
| street_address | string | Intentionally blank on row with PO box situation |
| city | string | |
| state | string | 2-letter |
| zip | string | |
| latitude | float | Blank on ~15% of rows to exercise geocoding |
| longitude | float | Blank on ~15% of rows to exercise geocoding |
| building_value | integer | Building replacement cost |
| contents_value | integer | Contents replacement cost |
| bi_value | integer | Business interruption / time element value |
| tiv | integer | Sum of building + contents + BI |
| construction_type | string | Text labels matching real SOV conventions |
| iso_construction_class | integer | 1-6 (ISO standard: 1=Frame, 6=Fire Resistive) |
| occupancy_type | string | Descriptive label |
| occupancy_code | integer | ISO/SIC-style 3-digit code |
| stories | integer | |
| year_built | integer | Range 1912-2019; some blank |
| sq_footage | integer | GFA in square feet |
| protection_class | integer | ISO 1-10; some blank (rural/unrated) |
| sprinklers | string | Y/N |
| alarm_type | string | None / Local / Central Station / Proprietary |
| roof_type | string | BUR / TPO / Metal / Membrane / Concrete |
| roof_year | integer | Year of last roof replacement |
| annual_premium | integer | In dollars |
| deductible | integer | Per-occurrence deductible |
| wind_deductible_pct | integer | Wind/hurricane deductible as % of TIV (FL/TX/NC only) |
| flood_zone | string | FEMA designation: X / AE / VE; blank if not mapped |
| wildfire_score | integer | 1-10 scale (analogous to Verisk FireLine); blank if n/a |
| distance_to_coast_mi | float | Miles to nearest coastline |
| notes | string | Underwriting annotations |

## Embedded Edge Cases

1. **Blank address** - CPL-2024-00106 Everglades Agriculture: no street_address, city="Hendry County FL", note says "PO BOX 1144". Tests geocoder robustness with county-only data.

2. **"Various" location** - CPL-2024-02001: blanket/floater policy with no individual location data. Common in real SOVs; tests how app handles unmappable rows.

3. **TIV concentration cluster** - CPL-2024-00101 (3 locations), CPL-2024-00102 (2 locations) all in Tampa 33603/33637. Combined TIV ~$35M within 1-mile radius. Tests accumulation analysis.

4. **VE flood zone** - CPL-2024-00105 (Naples high-rise) and CPL-2024-01302 (Miami Beach hotel) both in VE zone (coastal high-hazard). Tests flood zone filtering.

5. **Pre-1940 URM** - CPL-2024-00204 (Oakland 1924 warehouse): unreinforced masonry, ISO class 2, seismic note. Tests construction type filtering for CAT model prep.

6. **Harvey loss history** - CPL-2024-02501 location 2: notes field flags prior flood loss despite X flood zone. Real-world: properties flood outside mapped zones all the time.

7. **Missing lat/lon** - approximately 12 rows have blank coordinates. Spread across FL, TX, CA, GA to give geocoder a real workout.

8. **Highest TIV** - CPL-2024-01801 Las Vegas Strip: $191M TIV. Dominates any concentration or PML analysis. Tests large-value outlier handling.

9. **Special hazards** - grain elevator (CPL-2024-00304, explosion risk), ammonia refrigerant cold storage (CPL-2024-02101), wood products/sawmill (CPL-2024-00901). Three different special occupancy types.

10. **Zero BI value** - self-storage (CPL-2024-00104, CPL-2024-00504) and agricultural (CPL-2024-00106) have bi_value=0. Tests portfolio segmentation by time element.

## Geographic Distribution

| State | Locations | Total TIV (approx) |
|---|---|---|
| FL | 13 | ~$147M |
| CA | 10 | ~$270M |
| TX | 10 | ~$222M |
| NY | 5 | ~$181M |
| IL | 4 | ~$66M |
| CO | 3 | ~$54M |
| GA | 3 | ~$42M |
| Others | 17 | ~$200M+ |

## Realistic SOV Conventions Used
- Construction type uses text (not codes) - real SOVs are inconsistent
- Some rows have blank lat/lon (addresses only) - how most SOVs arrive
- TIV = sum of three components (not separately validated in data - intentional)
- Wind deductible % only on coastal states (FL, TX) - accurate to market practice
- Flood zone blank for inland properties - not every property has a FEMA determination on file
- Notes field used for underwriting flags - mirrors what underwriters actually write in systems
