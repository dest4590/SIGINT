from fastapi import FastAPI, HTTPException, Header, Depends
from pydantic import BaseModel
from typing import List, Optional, Dict
import aiosqlite
import json
import os
from datetime import datetime

app = FastAPI()
DB_PATH = "findings.db"
API_KEY = os.getenv("SIGINT_API_KEY", "your-secret-key")


async def verify_api_key(x_api_key: Optional[str] = Header(None)):
    if API_KEY and x_api_key != API_KEY:
        raise HTTPException(status_code=403, detail="Invalid API Key")
    return x_api_key


class Finding(BaseModel):
    id: str
    name: str
    rssi: int
    manufacturer_data: Dict[int, str]
    services: List[str]
    first_seen: str
    last_seen: str
    hit_count: int
    device_type: str
    beacon_type: Optional[str] = None
    distance_m: float
    is_connectable: bool
    rssi_history: List[int]
    signal_min: int
    signal_max: int
    signal_avg: float
    address_type: str
    beacon_uuid: Optional[str] = None
    beacon_major: Optional[int] = None
    beacon_minor: Optional[int] = None
    services_resolved: List[str]
    source_device_id: Optional[str] = "unknown"


@app.on_event("startup")
async def startup():
    async with aiosqlite.connect(DB_PATH) as db:
        await db.execute(
            """
            CREATE TABLE IF NOT EXISTS findings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT,
                name TEXT,
                rssi INTEGER,
                manufacturer_data TEXT,
                services TEXT,
                first_seen TEXT,
                last_seen TEXT,
                hit_count INTEGER,
                device_type TEXT,
                beacon_type TEXT,
                distance_m REAL,
                is_connectable BOOLEAN,
                rssi_history TEXT,
                signal_min INTEGER,
                signal_max INTEGER,
                signal_avg REAL,
                address_type TEXT,
                beacon_uuid TEXT,
                beacon_major INTEGER,
                beacon_minor INTEGER,
                services_resolved TEXT,
                source_device_id TEXT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )
        """
        )
        await db.commit()


@app.get("/")
async def root():
    return {"message": "SIGINT Backend with SQLite3"}


@app.post("/sync", dependencies=[Depends(verify_api_key)])
async def sync_findings(findings: List[Finding], since: Optional[str] = None):
    async with aiosqlite.connect(DB_PATH) as db:
        for finding in findings:
            await db.execute(
                """
                INSERT INTO findings (
                    device_id, name, rssi, manufacturer_data, services, 
                    first_seen, last_seen, hit_count, device_type, beacon_type, 
                    distance_m, is_connectable, rssi_history, signal_min, 
                    signal_max, signal_avg, address_type, beacon_uuid, 
                    beacon_major, beacon_minor, services_resolved, source_device_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
                (
                    finding.id,
                    finding.name,
                    finding.rssi,
                    json.dumps(finding.manufacturer_data),
                    json.dumps(finding.services),
                    finding.first_seen,
                    finding.last_seen,
                    finding.hit_count,
                    finding.device_type,
                    finding.beacon_type,
                    finding.distance_m,
                    finding.is_connectable,
                    json.dumps(finding.rssi_history),
                    finding.signal_min,
                    finding.signal_max,
                    finding.signal_avg,
                    finding.address_type,
                    finding.beacon_uuid,
                    finding.beacon_major,
                    finding.beacon_minor,
                    json.dumps(finding.services_resolved),
                    finding.source_device_id,
                ),
            )
        await db.commit()

        db.row_factory = aiosqlite.Row
        query = "SELECT * FROM findings"
        params = []
        if since:
            query += " WHERE timestamp > ?"
            params.append(since)
        query += " ORDER BY timestamp DESC"

        async with db.execute(query, params) as cursor:
            rows = await cursor.fetchall()
            return {
                "status": "success",
                "synced_count": len(findings),
                "new_findings": [dict(row) for row in rows],
            }


@app.get("/findings", dependencies=[Depends(verify_api_key)])
async def get_findings(since: Optional[str] = None):
    async with aiosqlite.connect(DB_PATH) as db:
        db.row_factory = aiosqlite.Row
        query = "SELECT * FROM findings"
        params = []
        if since:
            query += " WHERE timestamp > ?"
            params.append(since)
        query += " ORDER BY timestamp DESC"

        async with db.execute(query, params) as cursor:
            rows = await cursor.fetchall()
            return [dict(row) for row in rows]


@app.get("/db", dependencies=[Depends(verify_api_key)])
async def get_db_info():
    if os.path.exists(DB_PATH):
        size = os.path.getsize(DB_PATH)
        return {"database": DB_PATH, "size_bytes": size}
    return {"error": "Database not found"}
