# @reearth/okibi

The projection a service needs when it writes `tile.qk`, which is
[`okibi-qk`](../../crates/okibi-qk) compiled to wasm rather than a second
implementation of the same arithmetic.

```ts
import { qk8, quadkeyForTile } from "@reearth/okibi";

const qk = quadkeyForTile("geographic-tms", 14, 29108, 11439);
qk8(qk); // the cell a demand digest aggregates into
```

`pkg/` is built, not committed. Run `pnpm build` here, or
`scripts/build-wasm.sh` from the repository root, before anything imports this.

Planning is not in here yet. When it is, it arrives as more exports from this
same package.
