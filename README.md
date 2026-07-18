# Simulation<a name="simulation"></a>

Economy and military simulation

<!-- mdformat-toc start --slug=github --maxlevel=6 --minlevel=1 -->

- [Simulation](#simulation)
  - [Installation](#installation)
    - [Download Prebuilt Artifact](#download-prebuilt-artifact)
    - [Install from source](#install-from-source)
  - [Run From Source Without Installing](#run-from-source-without-installing)
  - [Run Terminal Viewer](#run-terminal-viewer)
  - [Run With Docker Compose](#run-with-docker-compose)
  - [Persistence](#persistence)
  - [Specification](#specification)
    - [Trusts](#trusts)
    - [Bases](#bases)
    - [Logic](#logic)
    - [API](#api)

<!-- mdformat-toc end -->

## Installation<a name="installation"></a>

To enable running via `simulation` in the terminal, there's two simple ways to install.

### Download Prebuilt Artifact<a name="download-prebuilt-artifact"></a>

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Dost-2-0-FK/simulation/releases/latest/download/simulation-installer.sh | sh
```

### Install from source<a name="install-from-source"></a>

Make sure you have [`cargo`](https://doc.rust-lang.org/cargo/getting-started/installation.html).

```sh
cargo install --git https://github.com/Dost-2-0-FK/simulation
```

## Run From Source Without Installing<a name="run-from-source-without-installing"></a>

Make sure you have [`cargo`](https://doc.rust-lang.org/cargo/getting-started/installation.html). To run the server from
source, with `debug` log-level

```shell
RUST_LOG=debug cargo run
```

To see all available endpoints, navigate to http://127.0.0.1:8080/swagger-ui/.

To run only the dependencies in Docker while iterating on the simulation server from the host:

```sh
RUN_CREDIT_EXCHANGER_SEED=true SEED_DROP_DATABASE=true docker compose up --force-recreate credit-exchanger mongodb
```

In another terminal:

```sh
RUST_LOG=debug cargo run
```

This uses the host-facing ports from `docker-compose.yml`: MongoDB on `localhost:27017` and credit-exchanger on
`http://127.0.0.1:18080`.

The seed file is mounted into the credit-exchanger container from `docker/credit-exchanger/db-seeding-example.json`.
Use `--force-recreate` when changing seed data so the credit-exchanger entrypoint runs the seed script again.
`SEED_DROP_DATABASE=true` clears only the credit-exchanger database first, avoiding state left by a partial seed.

## Run Terminal Viewer<a name="run-terminal-viewer"></a>

The workspace includes a `ratatui` terminal viewer that reads the simulation through the HTTP API. Its world bounds are
configured in `simulation-viewer.toml`; keep them in sync with the server's `[world]` values in `simulation.toml`.

```sh
cargo run -p simulation-viewer
```

By default it connects to `http://127.0.0.1:8080`. To use a different simulation server URL:

```sh
cargo run -p simulation-viewer -- http://127.0.0.1:8081
```

## Run With Docker Compose<a name="run-with-docker-compose"></a>

The Compose setup runs the stack as three services:

- MongoDB
- credit-exchanger
- simulation

Start the stack with Docker Compose:

```sh
docker compose up --build
```

Or with Podman Compose:

```sh
podman compose up --build
```

Then open http://127.0.0.1:8080/swagger-ui/.

If port `8080` is already in use on the host, change the `simulation` port mapping in `docker-compose.yml`, for example
to `"8081:8080"`.

The credit-exchanger image installs the latest published release with the GitHub release installer instead of compiling
credit-exchanger from source.

MongoDB data is stored in the named Compose volume `simulation_mongo-data`. To remove the stack and the persisted
database:

```sh
docker compose down --volumes
```

To seed the credit-exchanger database on startup:

```sh
RUN_CREDIT_EXCHANGER_SEED=true docker compose up --build
```

To run in the background:

```sh
docker compose up --build -d
```

## Persistence<a name="persistence"></a>

Runtime state is loaded from MongoDB on startup. All read endpoints are served from in-memory state. Mutations update
only the in-memory state immediately; the full state is flushed to MongoDB periodically by a background task.

Configure MongoDB and the persistence interval in `simulation.toml`:

```toml
[persistence]
uri = "mongodb://localhost:27017"
database = "simulation"
interval_seconds = 30
```

Configure `bank_user_id` to the credit-exchanger user that receives structure build costs:

```toml
bank_user_id = "bank"
```

Configure the service URLs used for authorization and credit exchange:

```toml
[env]
auth_service_url = "http://127.0.0.1:18081"
credit_exchange_url = "http://127.0.0.1:18080"
```

Financed trust and base creation is checked once per financier at
`POST /api/users/{financierId}/financing/verify` before payment is booked. Every financier must approve the request.

Configure combat loot factors for destroyed structures and killed units in `simulation.toml`. Omitted factors default to
`0`.

```toml
[combat.loot_factors]
money = 0.5
resources = { lithium = 0.5, iron = 0.5 }
```

## Specification<a name="specification"></a>

There are four fundamental types:

- Placements (associated with a zone)
- Trust (associated with a zone)
- Base (associated with a zone and a bloc)
- military unit (associated with a base and a bloc) *note: each zone is associated with a bloc*.

### Trusts<a name="trusts"></a>

- Are created on a placement
- Cost "money" (`Float`) and "resources" (`Dict<str, Float>`)
- Are "paid" by the zone the placement is associated with (at least `50%`), and potentially by a financier (max `50%`)
- Payments are made to the `credit-exchanger`:
  - `CREDIT-EXCHANGER-SERVICE/api/credits/book?id=<unique_id>&receiver=<receiver_id>&value=<value>`
  - `CREDIT-EXCHANGER-SERVICE/api/resource/book?id=<unique_id>&receiver=<receiver_id>&value=<n-tuple>`
  - If financed, also book from the financier
  - If all requests succeed: `/api/units/add?id=<trust_id>`
- Generate "money" and *one* "resource"
- Resource production is based on a configured value, negatively influenced by close enemy military units, and updated
  hourly.
  - Resource values are configured via `[trust_production.resources] <resource> = <value>`.
- Money production has a configured base value per produced resource:
  `[trust_production.money_per_resource] <resource> = <value>`.
  - The credit-exchanger is queried for the total balance of that resource across all users except the configured bank
    user.
  - Final money production is `base value / (total existing resource units + 1)`.
  - `CREDIT-EXCHANGER-SERVICE/api/units/set_credit_production?id<trust_id>&value=<value>`
  - `CREDIT-EXCHANGER-SERVICE/api/units/set_resource_production?id<trust_id>&resource=<resource>&value=<value>`
  - The resources generated are additionally influenced by the spent resources of that unit and the current production
    of that resource unit *in the other bloc*.
- One subscription is added for "recourse" and one for "money"
  - `CREDIT-EXCHANGE-SERVICE/api/subscription/add?id=<trust_id>` (JSON payload
    `{'id':'<zone_id|financier_id>', 'receiver': 'receiver_id', 'value': <value>, 'type': '<type>', 'priority': <priority>}`)
  - If only paid by zone, `value=100`, if paid by zone and financier, two subscriptions are created: f.e, one with
    `value=70`, one with `value=30`.
  - `CREDIT-EXCHANGE-SERVICE/api/subscription/resource/add?id=<trust_id>&resource=<resource>` (JSON payload
    `{'id':'<zone_id|financier_id>', 'receiver': 'receiver_id', 'value': <value>, 'type': '<type>', 'priority': <priority>}`)
  - If only paid by zone, `value=100`, if paid by zone and financier, two subscriptions are created: f.e, one with
    `value=70`, one with `value=30`.
  - Trust can be disabled or removed
  - Trusts can be destroyed by enemy targets

### Bases<a name="bases"></a>

- Are created on a placement
- Cost "money" (`Float`) and "resources" (`Dict<str, Float>`)
- Are "paid" by the bloc creating the base (at least `50%`), and potentially by a financier (max `50%`)
- Payments are made to the `credit-exchanger`:
  - `CREDIT-EXCHANGER-SERVICE/api/credits/book?id=<unique_id>&receiver=<receiver_id>&value=<value>`
  - `CREDIT-EXCHANGER-SERVICE/api/resource/book?id=<unique_id>&receiver=<receiver_id>&value=<n-tuple>`
  - If both requests succeed: `/api/units/add?id=<base_id>`
- Generate "money" based on enemy military units killed by military units from this base (`production_count`)
- The `production_count` is posted to the `credit-exchanger` hourly and cleared afterwards:
  - `CREDIT-EXCHANGER-SERVICE/api/units/set_credit_production?id<base_id: str>&value=<value: float>`
- A subscription is added:
  - `CREDIT-EXCHANGE-SERVICE/api/subscription/add?id=<base_id>` (JSON payload
    `{'id':'<base_id|financier_id>', 'receiver': 'receiver_id', 'value': <value>, 'type': '<type>', 'priority': <priority>}`)
  - If only paid by bloc, `value=100`, if paid by bloc and financier, two subscriptions are created: f.e, one with
    `value=70`, one with `value=30`.
- Bases can be disabled (`PATCH /api/bases/{id}` with body `{enabled: false}`) or removed
  (`/api/base/remove?id=<base_id>`)
- Bases can be destroyed by enemy targets
- Bases can be prioritised (`PATCH /api/bases/{id}` with body `{prioritized: <true|false>}`)
- Bases can define a target (`PATCH /api/bases/{id}` with body `{target: <trust|base|unit>}`)

### Logic<a name="logic"></a>

- When `POST /api/bases/publish-production` is called, accumulated base loot is published to the credit service.
- When `POST /api/trusts/publish-production` is called, each trust's resource production and dynamically discounted
  money income are published to the credit service.
- When `POST /api/units/produce` is called, military units are produced in bases:
  - Get the blocs' hourly income: `CREDIT-EXCHANGE-SERVICE/api/credits/hourly?id=<bloc_id>`
  - Each bloc can define what percentage of the hourly income shall be used for creating military units (can be set by
    `PATCH /api/blocs/{id}` with body `{militaryExpense: <value: int>}`)
  - The result of `hourly_income * military_expense/100` is used to create military units until the designated money is
    spent
    - 2 for enabled prioritised bases
    - 1 for enabled bases
    - 0 for disabled bases
- Every minute, units move towards their target: closet enemy trust, base or unit (depending on the target defined in
  the base the unit was created at)
- If two units of different blocs meet, always roll the dice for *both* units to decide which unit is destroyed: each
  bloc has a chance (0-1) (can be set by calling the corresponding endpoint, see below)
- If a unit is killed, increase the `production_count` of the killer's base by the configured loot factors applied to
  the killed unit's cost
- If a unit meets an enemy base/trust, it stays there until at least X units "attack" the base/trust, and no enemy units
  are in a radius of Y -> in that case, the base/trust is destroyed
- If enemy units are in radius of Y, the unity prioritizes attacking those units.
- If a base/trust is destroyed, increase the `production_count` of the attacking units' bases by the configured loot
  factors applied to the destroyed base/trust's cost

### API<a name="api"></a>

Every request is issued by a user, and a map defines which requests are allowed for that user or whether f.e. *all*
trusts are returned or only a subset (only associated by block/zone financed by individual)

- `GET /api/bases` (returns bases and associated bloc and financier and the percentage the base was financed by the
  financier)
- `GET /api/bases/{id}`
- `POST /api/bases` (payload:
  `{placementId: <placement id>, payment: [{financierId: <financier_id (str)>, share: <value (float, 0-1)>}]}`)
- `POST /api/bases/publish-production` (publishes accumulated base loot)
- `PATCH /api/bases/{id}` (payload:
  `{(optional) enabled: <true|false>, (optional) prioritized: <true|false>, (optional) target: <trust|base|unit>}`)
- `GET /api/blocs`
- `GET /api/blocs/{id}` -> response should contain chance and military expense
- `PATCH /api/blocs/{id}` payload: `{(optional) chance: <value (float)>, (optional) militaryExpense: <value (int)>}`
- `GET /api/placements` (returns placements and associated zone)
- `GET /api/placements/{id}`
- `GET /api/trusts` (returns trusts and associated zone and financier and the percentage the trust was financed by the
  financier)
- `GET /api/trusts/{id}`
- `POST /api/trusts/publish-production` (publishes configured trust production)
- `POST /api/trusts` (payload:
  `{placementId: <placement id>, resource: <resource name>, payment: [{financierId: <financier_id (str)>, share: <value (float, 0-1)>}]}`)
- `GET /api/units` (returns units and associated Bloc and Base)
- `POST /api/units/produce` (produces military units)
- `GET /api/zones`
