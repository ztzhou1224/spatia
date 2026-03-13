"""
Generate a synthetic Washington State business dataset with 1000 rows.
Run with: python scripts/gen_synthetic_businesses.py
"""

import csv
import random
from datetime import date, timedelta

random.seed(42)

OUTPUT_PATH = "/Users/zhaotingzhou/Projects/spatia/data/synthetic_businesses_1k.csv"

# --- Cities with approximate lat/lon centroids and zip ranges ---------------
CITIES = [
    ("Seattle",      47.61, -122.33, [98101, 98102, 98103, 98104, 98105, 98106, 98107, 98108, 98109, 98112, 98115, 98116, 98117, 98118, 98119, 98121, 98122, 98125, 98126, 98133, 98134, 98136, 98144, 98146]),
    ("Bellevue",     47.61, -122.20, [98004, 98005, 98006, 98007, 98008]),
    ("Tacoma",       47.25, -122.44, [98401, 98402, 98403, 98404, 98405, 98406, 98407, 98408, 98409, 98418]),
    ("Redmond",      47.67, -122.12, [98052, 98053]),
    ("Kirkland",     47.68, -122.21, [98033, 98034]),
    ("Renton",       47.49, -122.19, [98055, 98056, 98057, 98058, 98059]),
    ("Kent",         47.38, -122.23, [98030, 98031, 98032, 98035, 98042]),
    ("Everett",      47.98, -122.20, [98201, 98203, 98204, 98208]),
    ("Federal Way",  47.32, -122.31, [98003, 98023]),
    ("Olympia",      47.04, -122.90, [98501, 98502, 98503, 98506]),
    ("Spokane",      47.66, -117.43, [99201, 99202, 99203, 99204, 99205, 99206, 99207, 99208]),
    ("Vancouver",    45.63, -122.67, [98660, 98661, 98662, 98663, 98664, 98665]),
    ("Bellingham",   48.75, -122.48, [98225, 98226, 98229]),
    ("Yakima",       46.60, -120.51, [98901, 98902, 98903, 98908]),
    ("Kennewick",    46.21, -119.14, [99336, 99337, 99338]),
]

CITY_NAMES = [c[0] for c in CITIES]
CITY_MAP   = {c[0]: c for c in CITIES}

# --- Categories and subcategories -------------------------------------------
CATEGORIES = {
    "Technology":             ["Software", "Hardware", "Cloud Services", "Cybersecurity"],
    "Healthcare":             ["Medical Devices", "Pharmaceuticals", "Clinic Services", "Telehealth"],
    "Retail":                 ["Apparel", "Electronics", "Home Goods", "Sporting Goods"],
    "Manufacturing":          ["Aerospace", "Food Processing", "Metal Fabrication"],
    "Food & Beverage":        ["Restaurant", "Catering", "Brewery", "Bakery"],
    "Professional Services":  ["Consulting", "Legal", "Accounting", "Marketing"],
}

# --- Name building blocks ---------------------------------------------------
PREFIXES   = ["Pacific", "Northwest", "Cascade", "Puget", "Olympic", "Summit", "Evergreen",
               "Pioneer", "Columbia", "Rainier", "Soundside", "Alpine", "Horizon", "Coastal",
               "Metro", "Urban", "Peak", "Bay", "Harbor", "Cedar"]
MIDWORDS   = ["Tech", "Health", "Data", "Systems", "Solutions", "Services", "Works",
               "Dynamics", "Labs", "Group", "Partners", "Innovations", "Analytics",
               "Design", "Ventures", "Connect", "Logic", "Craft", "Bridge", "Edge"]
SUFFIXES   = ["Inc", "Corp", "LLC", "Co", "Group", "Associates", "Enterprises",
               "Holdings", "Agency", "Studio", "Consulting", "Industries"]
STREET_TYPES = ["Ave", "St", "Blvd", "Rd", "Dr", "Way", "Ln", "Pl", "Ct", "Pkwy"]
STREET_NAMES = ["Main", "Oak", "Maple", "Pine", "Cedar", "Elm", "Washington", "Park",
                "Lake", "Hill", "River", "Forest", "Valley", "Sunrise", "Sunset",
                "Pacific", "Market", "Commerce", "Industrial", "Harbor", "Ridge",
                "Meadow", "Spring", "Canyon", "Willow", "Cherry", "Division",
                "Broadway", "Lincoln", "Madison", "Jefferson", "Adams", "Monroe"]

NAME_PATTERNS = [
    lambda p, m, s: f"{p} {m} {s}",
    lambda p, m, s: f"{p} {s}",
    lambda p, m, s: f"{m} {s}",
    lambda p, m, s: f"The {p} {m}",
    lambda p, m, s: f"{p} & {m} {s}",
]

def random_business_name():
    p = random.choice(PREFIXES)
    m = random.choice(MIDWORDS)
    s = random.choice(SUFFIXES)
    pattern = random.choice(NAME_PATTERNS)
    return pattern(p, m, s)

def random_address():
    number = random.randint(100, 9999)
    street = random.choice(STREET_NAMES)
    stype  = random.choice(STREET_TYPES)
    return f"{number} {street} {stype}"

def random_date(start: date, end: date) -> str:
    delta = (end - start).days
    return (start + timedelta(days=random.randint(0, delta))).strftime("%Y-%m-%d")

def random_lat_lon(city_name: str) -> tuple[float, float]:
    _, clat, clon, _ = CITY_MAP[city_name]
    lat = round(clat + random.uniform(-0.06, 0.06), 6)
    lon = round(clon + random.uniform(-0.06, 0.06), 6)
    # Clamp to WA state bounds
    lat = max(46.0, min(48.9, lat))
    lon = max(-124.0, min(-117.0, lon))
    return lat, lon

# --- Generate rows ----------------------------------------------------------
INSPECTION_START = date(2020, 1, 1)
INSPECTION_END   = date(2025, 12, 31)

rows = []
for i in range(1, 1001):
    city_name = random.choice(CITY_NAMES)
    _, _, _, zips = CITY_MAP[city_name]
    category    = random.choice(list(CATEGORIES.keys()))
    subcategory = random.choice(CATEGORIES[category])
    lat, lon    = random_lat_lon(city_name)

    rows.append({
        "id":                  i,
        "name":                random_business_name(),
        "address":             random_address(),
        "city":                city_name,
        "state":               "WA",
        "zip":                 random.choice(zips),
        "category":            category,
        "subcategory":         subcategory,
        "annual_revenue":      random.randint(50_000, 50_000_000),
        "employee_count":      random.randint(1, 5000),
        "founded_year":        random.randint(1950, 2024),
        "lat":                 lat,
        "lon":                 lon,
        "is_active":           "true" if random.random() < 0.9 else "false",
        "last_inspection_date": random_date(INSPECTION_START, INSPECTION_END),
    })

FIELDNAMES = [
    "id", "name", "address", "city", "state", "zip",
    "category", "subcategory", "annual_revenue", "employee_count",
    "founded_year", "lat", "lon", "is_active", "last_inspection_date",
]

with open(OUTPUT_PATH, "w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=FIELDNAMES)
    writer.writeheader()
    writer.writerows(rows)

print(f"Wrote {len(rows)} rows to {OUTPUT_PATH}")
