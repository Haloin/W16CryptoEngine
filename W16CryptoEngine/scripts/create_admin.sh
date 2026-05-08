
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL must be set}"
: "${ADMIN_EMAIL:?ADMIN_EMAIL must be set}"

psql "$DATABASE_URL" -c "UPDATE users SET is_admin = true WHERE email = '$ADMIN_EMAIL';"
echo "admin flag set for $ADMIN_EMAIL"
