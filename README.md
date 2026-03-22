# Simulation<a name="simulation"></a>

Economy and military simulation

<!-- mdformat-toc start --slug=github --maxlevel=6 --minlevel=1 -->

- [Simulation](#simulation)
  - [Installation](#installation)
  - [Specification](#specification)
    - [Trusts](#trusts)
    - [Bases](#bases)
    - [Logic](#logic)
    - [API](#api)

<!-- mdformat-toc end -->

## Installation<a name="installation"></a>

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
- The amount of both is based on a fixed value, negatively influenced by close enemy military units and updated hourly
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
- Bases can be disabled or removed (`/api/base/disable?id=<base_id>`, `/api/base/remove?id=<base_id>`)
- Bases can be destroyed by enemy targets
- Bases can be prioritised (`POST /api/base/prioritise?id=<base_id: str>&value=<true|false>`)
- Bases can define a target (`POST /api/base/target?id=<base_id>&target=<trust|base|unit>`)

### Logic<a name="logic"></a>

- Every hour, credit-/resource-production of trusts and bases is updated (see above)
- Every hour, military units are produced in bases:
  - Get the blocs' hourly income: `CREDIT-EXCHANGE-SERVICE/api/credits/hourly?id=<bloc_id>`
  - Each bloc can define what percentage of the hourly income shall be used for creating military units (can be set by
    `POST /api/bloc/military_expense?id=<bloc_id: str>&value=<value: int>`)
  - The result of `hourly_income * military_expense/100` is used to create military units until the designated money is
    spent
    - 2 for enabled prioritised bases
    - 1 for enabled bases
    - 0 for disabled bases
- Every minute, units move towards their target: closet enemy trust, base or unit (depending on the target defined in
  the base the unit was created at)
- If two units of different blocs meet, always roll the dice for *both* units to decide which unit is destroyed: each
  bloc has a chance (0-1) (can be set by: `POST /api/bloc/chance?id=<bloc_id: str>&chance=<value: float>`)
- If a unit is killed, increase the `production_count` of the base of the enemy unit by half the amount of money spent
  to create the unit
- If a unit meets an enemy base/trust, it stays there until at least X units "attack" the base/trust, and no enemy units
  are in a radius of Y -> in that case, the base/trust is destroyed
- If enemy units are in radius of Y, the unity prioritizes attacking those units.
- If a base/trust is destroyed, increase the `production_count` of the base of the enemy unit by half the amount of
  money spent to create the base/trust

### API<a name="api"></a>

Every request is issued by a user, and a map defines which requests are allowed for that user or whether f.e. *all*
trusts are returned or only a subset (only associated by block/zone financed by individual)

- `GET /api/units` (returns units and associated Bloc and Base)
- `GET /api/placements` (returns placements and associated zone)
- `GET /api/trusts` (returns trusts and associated zone and financier and the percentage the trust was financed by the
  financier)
- `GET /api/bases` (returns bases and associated bloc and financier and the percentage the trust was financed by the
  financier)
- `GET /api/bloc/military_expense`
- `GET /api/bloc/chance`
- `POST /api/trust?placement=<placement_id>` (JSON payload:
  `{financier: <financier_id: str>, percentage: <value: int>}`)
- `POST /api/base?placement=<placement_id>` (JSON payload: `{financier: <financier_id: str>, percentage: <value: int>}`)
- `POST /api/bloc/military_expense?id=<bloc_id: str>`
- `POST /api/bloc/chance?id=<bloc_id: str>`
- `POST /api/base/prioritise?id=<base_id: str>&value=<true|false>`
- `POST /api/base/target?id=<base_id>&target=<trust|base|unit>`
