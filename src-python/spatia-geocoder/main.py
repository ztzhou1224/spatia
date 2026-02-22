import os
import argparse
import json

from fastapi import FastAPI
from geopy.geocoders import Nominatim
from pydantic import BaseModel
from typing import List, Optional
import uvicorn

app = FastAPI()
geocoder = Nominatim(user_agent="spatia-geocoder")


class GeocodeRequest(BaseModel):
    addresses: List[str]


class GeocodeResult(BaseModel):
    address: str
    lat: Optional[float]
    lon: Optional[float]
    status: Optional[str] = None
    error: Optional[str] = None


@app.post("/geocode")
def geocode(payload: GeocodeRequest) -> List[GeocodeResult]:
    return geocode_addresses(payload.addresses)


def geocode_addresses(addresses: List[str]) -> List[GeocodeResult]:
    debug = geocoder_debug_enabled()
    results: List[GeocodeResult] = []
    for address in addresses:
        try:
            location = geocoder.geocode(address)
        except Exception as exc:
            results.append(
                GeocodeResult(
                    address=address,
                    lat=None,
                    lon=None,
                    status="error" if debug else None,
                    error=f"{type(exc).__name__}: {exc}" if debug else None,
                )
            )
            continue
        if location is None:
            results.append(
                GeocodeResult(
                    address=address,
                    lat=None,
                    lon=None,
                    status="not_found" if debug else None,
                    error=None,
                )
            )
            continue
        results.append(
            GeocodeResult(
                address=address,
                lat=location.latitude,
                lon=location.longitude,
                status="ok" if debug else None,
                error=None,
            )
        )
    return results


def geocoder_debug_enabled() -> bool:
    value = os.environ.get("SPATIA_GEOCODER_DEBUG", "").strip().lower()
    return value in {"1", "true", "yes", "on"}


def run() -> None:
    parser = argparse.ArgumentParser(prog="spatia-geocoder")
    parser.add_argument("addresses", nargs="*")
    parser.add_argument("--serve", action="store_true")
    args = parser.parse_args()

    if not args.serve:
        if not args.addresses:
            parser.error("Provide at least one address, or use --serve")
        results = geocode_addresses(args.addresses)
        print(json.dumps([result.model_dump() for result in results]))
        return

    port = int(os.environ.get("SPATIA_GEOCODER_PORT", "7788"))
    uvicorn.run(app, host="127.0.0.1", port=port, log_level="info")


if __name__ == "__main__":
    run()
