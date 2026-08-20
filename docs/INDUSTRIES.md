# Supported industries

`Event.category` is a free-text field on-chain; the backend enforces
a closed set of twelve values so the marketplace and dashboard can
filter and label consistently:

Concerts, Flights, Sports, Festivals, Conferences, Bus companies,
Movie theaters, Museums, Tourist attractions, Public transport,
Universities, Corporate events.

Adding a thirteenth industry requires no contract change — only a
backend enum update (`Industry` in
[`StellarTickets/backend`](https://github.com/StellarTickets/backend)'s
Prisma schema) and a frontend label.
