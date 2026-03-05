#!/usr/bin/env bash
# Sidekick full-image entrypoint
# Starts PostgreSQL → Redis → Sidekick server in sequence.
set -euo pipefail

PG_DATA="${PGDATA:-/var/lib/postgresql/data}"
PG_USER="${POSTGRES_USER:-sidekick}"
PG_PASSWORD="${POSTGRES_PASSWORD:-sidekick}"
PG_DB="${POSTGRES_DB:-sidekick}"

# ---------------------------------------------------------------------------
# 1. Initialise PostgreSQL data directory (first boot only)
# ---------------------------------------------------------------------------
if [ ! -f "$PG_DATA/PG_VERSION" ]; then
  echo "[init] Initialising PostgreSQL data directory..."
  su -s /bin/sh postgres -c "initdb -D '$PG_DATA' --username='$PG_USER' --pwfile=<(echo '$PG_PASSWORD') --auth-host=md5 --auth-local=trust"
  echo "[init] PostgreSQL initialised."
fi

# ---------------------------------------------------------------------------
# 2. Start PostgreSQL
# ---------------------------------------------------------------------------
echo "[init] Starting PostgreSQL..."
su -s /bin/sh postgres -c "pg_ctl -D '$PG_DATA' -l /var/log/postgresql.log start -w"

# Ensure database exists
su -s /bin/sh postgres -c "psql -U '$PG_USER' -tc \"SELECT 1 FROM pg_database WHERE datname='$PG_DB'\" | grep -q 1 || psql -U '$PG_USER' -c \"CREATE DATABASE $PG_DB;\""
echo "[init] PostgreSQL ready."

# ---------------------------------------------------------------------------
# 3. Start Redis
# ---------------------------------------------------------------------------
echo "[init] Starting Redis..."
redis-server --daemonize yes --logfile /var/log/redis.log --appendonly yes
echo "[init] Redis ready."

# ---------------------------------------------------------------------------
# 4. Export connection strings for the server
# ---------------------------------------------------------------------------
export DATABASE_URL="postgres://${PG_USER}:${PG_PASSWORD}@127.0.0.1/${PG_DB}"
export REDIS_URL="redis://127.0.0.1:6379"
export PUBLIC_DIR="${PUBLIC_DIR:-/app/public}"

# SDK_KEY and PORT are passed in via docker run -e / docker-compose env
echo "[init] Starting Sidekick server..."
exec sidekick-server
