# SIGINT Backend

simple python backend for saving all bluetooth devices, its optional

## how to configure?

The backend requires an API key for most endpoints. You can set it using the `SIGINT_API_KEY` environment variable.

- **Default Key**: `your-secret-key` (if not set)

## what endpoints are available?

- `GET /`: Root endpoint (no auth)
- `POST /sync`: Sync findings (requires `X-API-Key` header)
- `GET /findings`: Retrieve findings (requires `X-API-Key` header)
- `GET /db`: Database info (requires `X-API-Key` header)

## how to run?

Run the backend using `run_backend.bat` or:

```bash
pip install -r requirements.txt
uvicorn app:app --host 0.0.0.0 --port 8000 --reload
```
