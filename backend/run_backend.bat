@echo off
cd /d %~dp0
:: Set your API key here or via environment variable
set SIGINT_API_KEY=your-secret-key
pip install -r requirements.txt
uvicorn app:app --host 0.0.0.0 --port 8000 --reload
pause
