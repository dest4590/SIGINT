#!/usr/bin/env python3
import json
import sys
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

COMPANY_ID_URL = (
    "https://raw.githubusercontent.com/NordicSemiconductor/bluetooth-numbers-database/master/v1/company_ids.json"
)
SERVICE_UUID_URL = (
    "https://raw.githubusercontent.com/NordicSemiconductor/bluetooth-numbers-database/master/v1/service_uuids.json"
)

ROOT = Path(__file__).resolve().parent

FILES = {
    "company_ids.json": COMPANY_ID_URL,
    "service_uuids.json": SERVICE_UUID_URL,
}

USER_AGENT = "sigint-db-updater/1.0"


def fetch(url: str) -> str:
    request = Request(url, headers={"User-Agent": USER_AGENT})
    with urlopen(request, timeout=30) as response:
        if response.status != 200:
            raise ValueError(f"HTTP {response.status} while fetching {url}")
        return response.read().decode("utf-8")


def validate_json(text: str, filename: str) -> None:
    try:
        json.loads(text)
    except json.JSONDecodeError as exc:
        raise ValueError(f"Invalid JSON for {filename}: {exc}") from exc


def write_file(filename: str, content: str) -> None:
    path = ROOT / filename
    path.write_text(content, encoding="utf-8")
    print(f"Updated {path}")


def main() -> int:
    print("[SYSTEM] Updating databases...")

    for filename, url in FILES.items():
        print(f"Fetching {filename} from {url}")
        try:
            content = fetch(url)
            validate_json(content, filename)
            write_file(filename, content)
        except (HTTPError, URLError, ValueError) as exc:
            print(f"[ERROR] Failed to update {filename}: {exc}")
            return 1

    print("[SYSTEM] Databases updated successfully.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
