# Soroban Forge Next.js example

The example app under `src/app` exercises the escrow TypeScript client against
the configured testnet contract.

## Local demo state

The `Local demo state` panel controls browser-only form values used while
testing the demo:

- `soroban-forge/demo-state@1` is the versioned JSON schema.
- `soroban-forge:nextjs-demo-state:v1` is the `localStorage` key.
- Export produces a JSON document with `schema`, `kind`, `version`, `label`,
  `exportedAt`, and `state`.
- `state` is `null` when there is no saved demo state.
- The panel shows the current validated state, its schema/version, and the
  storage key before any export.

Exports contain only local form values. They do not include escrow IDs created
by live transactions, wallet keys, signed transaction payloads, network
configuration, or contract state. The UI labels exported data as local browser
demo state.

Stored JSON and exported documents are validated before use. Validation checks
schema, version, unknown fields, secret-looking values, address/contract-id
shape, integer fields, and ISO timestamps. Malformed data, a different schema,
or a different version are rejected. Invalid stored data is removed and the
demo falls back to defaults.

`Import local demo state JSON` validates a downloaded document before applying
or persisting it. Rejected imports leave stored state unchanged. An empty export
clears saved demo state. Legacy stored objects that include old escrow-id
fields remain readable; those transient fields are ignored and never exported.

`Reset local demo state` requires confirmation. It clears the demo form state
and restores defaults. It does not disconnect wallets, change the network or
contract, or modify on-chain data. The dialog can be cancelled with `Escape`.

## Development

```sh
npm install
npm run dev
```

Verification:

```sh
npm run lint
npm run test
npm run build
```
