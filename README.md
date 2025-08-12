# Simulation
Economy and military simulation

## Installation

## Specification 
There are four fundamental types: 
- Placements (associated with a zone)
- Trust (associated with a zone)
- Base (associated with a zone and a bloc)
- military unit (associated with a base and a bloc)

Trusts: 
- Are created on a placement
- Cost "money" (`Float`) and "resources" (`Dict<str, Float>`)
- Are "paid" by the zone the placement is associated with, and potentially a financier
- Payments are made to the `credit-exchanger`:
  - `/api/credits/book?id=<unique_id>&receiver=<receiver_id>&value=<value>`
  - `/api/resource/book?id=<unique_id>&receiver=<receiver_id>&value=<n-tuple>`
- Generates "money" and "resources" based on a fixed value and is negatively influenced by close enemy military units
- For Zones: Generated "money" and "resources" are handled by adding/updating `subscriptions` in the `credit` manager:
  - `GET /api/subscriptions?id=<zone_id>` (where JSON payload defines the subscription) when the trust is created
  - `GET /api/subscriptions/update?id=<zone_id>&value=<current 
