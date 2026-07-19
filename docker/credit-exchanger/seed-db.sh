#!/usr/bin/env bash
set -euo pipefail

SEED_FILE="${1:-src/credit-exchanger.json}"
DB_URI="${DB_URI:-mongodb://localhost:27017}"
DB_DATABASE="${DB_DATABASE:-credit_exchanger}"
SEED_DROP_DATABASE="${SEED_DROP_DATABASE:-false}"

if [[ ! -f "$SEED_FILE" ]]; then
  echo "Seed file not found: $SEED_FILE" >&2
  exit 1
fi

if ! command -v mongosh >/dev/null 2>&1; then
  echo "mongosh is required but was not found in PATH." >&2
  exit 1
fi

export SEED_FILE DB_DATABASE SEED_DROP_DATABASE

mongosh "$DB_URI/$DB_DATABASE" --quiet <<'MONGOSH'
const fs = require('fs');

const seedFile = process.env.SEED_FILE;
const shouldDropDatabase = String(process.env.SEED_DROP_DATABASE || 'false').toLowerCase() === 'true';

let payload;
try {
  payload = JSON.parse(fs.readFileSync(seedFile, 'utf8'));
} catch (err) {
  print(`Failed to read/parse seed file ${seedFile}: ${err.message}`);
  quit(1);
}

function ensureArray(value) {
  return Array.isArray(value) ? value : [];
}

function normalizeSubscription(subscription) {
  return {
    id: String(subscription.id || ObjectId().toString()),
    receiver: String(subscription.receiver || ''),
    value: Number(subscription.value || 0),
    subscription_type: String(subscription.subscription_type || 'contract'),
    priority: Number(subscription.priority || 0),
    credit_type: String(subscription.credit_type || 'money'),
  };
}

function normalizeCredit(credit) {
  const raw = credit || {};
  return {
    total: Number(raw.total || 0),
    last_day_average: Number(raw.last_day_average || 0),
    subscriptions: ensureArray(raw.subscriptions).map(normalizeSubscription),
    history: ensureArray(raw.history).map((x) => Number(x || 0)),
  };
}

function normalizeResources(resources) {
  const raw = resources || {};
  const normalized = {};

  for (const [name, credit] of Object.entries(raw)) {
    normalized[String(name)] = normalizeCredit(credit);
  }

  return normalized;
}

function normalizeUser(userType, user) {
  const base = {
    _id: String(user.id),
    id: String(user.id),
    type: userType,
    credit: normalizeCredit(user.credit),
  };

  if (userType === 'Bloc' || userType === 'Zone') {
    base.resources = normalizeResources(user.resources);
  }

  return base;
}

const users = [];

for (const bloc of ensureArray(payload.blocs)) users.push(normalizeUser('Bloc', bloc));
for (const zone of ensureArray(payload.zones)) users.push(normalizeUser('Zone', zone));
for (const individual of ensureArray(payload.individuals)) users.push(normalizeUser('Individual', individual));
for (const unit of ensureArray(payload.units)) users.push(normalizeUser('Unit', unit));

if (shouldDropDatabase) {
  db.dropDatabase();
  print(`Dropped database ${db.getName()} before seeding.`);
}

let insertedOrUpdated = 0;
for (const user of users) {
  db.users.replaceOne({ _id: user._id }, user, { upsert: true });
  insertedOrUpdated += 1;
}

print(`Seed complete. Upserted ${insertedOrUpdated} users into ${db.getName()}.users`);
MONGOSH

echo "Seeding finished. DB_DATABASE=$DB_DATABASE SEED_FILE=$SEED_FILE"
