// Writes one tile-demand event per tile request, from inside a service.
//
// Services should not have to know how the log backend lays the vocabulary
// out, nor rebuild the epoch strings that go into their own cache keys. Both
// of those are exactly where a service drifts away from the spec without
// noticing, so both belong here rather than in each service.
//
// Nothing here yet — see `spec/tile-demand.md` and `spec/bindings/wae-1.md`.

export {};
