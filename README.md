# archer-cli

Command-line interface for [Archer Protocol](https://archer.exchange) — a fully
on-chain trading venue on Solana that aggregates sovereign market-maker
orderbooks into a single atomic execution layer.

One binary, `archer`, for creating markets, inspecting live state, and managing
the markets and maker books you own.

```bash
cargo install --path .
archer --help
```

Connection and signer default to your Solana CLI config, so if `solana config
get` works, `archer` works. Override per-invocation with `-u` and `-k`.

---

## Discovering what's out there

Everything starts with finding a market. `market list` asks the RPC for every
market account the program owns and shows the ones currently trading.

```bash
archer market list
```

```
═══ Active Markets ═══

+----------------------------------------------+---------------------------+--------+-------+---------+
| Market                                       | Base mint                 | Status | Maker | Taker   |
+======================================================================================================+
| u8tnfCb1JSSghuNFquQ2beStYgAN1kmd1f1Lhxbaec4  | So1111…1111112 (SOL)      | Active | 0 ppm | 0 ppm   |
| 14biegnyB3acwJeee6ahyvUSM5t4qcCNFgxpw6eYerDj | MRNAzXz…LUCT              | Active | 0 ppm | 200 ppm |
+----------------------------------------------+---------------------------+--------+-------+---------+

  27 inactive market(s) hidden. Pass --all to include them.
```

Paused and closed markets are hidden by default, because a paused market can't
be traded and listing it alongside live ones invites mistakes. When you do want
the full picture — auditing, or checking whether a market you created is still
running:

```bash
archer market list --all
```

The filtering happens RPC-side via a memcmp filter on the account
discriminator, so this stays fast as the program grows.

---

## Inspecting a market

With a market address in hand, `observe` answers progressively more specific
questions. Every one is read-only and needs no keypair.

**The market's configuration and accrued fees:**

```bash
archer observe market --market u8tnfCb1JSSghuNFquQ2beStYgAN1kmd1f1Lhxbaec4
```

Shows mints, vaults, lot and tick sizes, decimals, the fee config, current
status, and uncollected versus collected protocol fees.

**The aggregated book — what a taker would actually hit:**

```bash
archer observe liquidity --market u8tnfCb1JSSghuNFquQ2beStYgAN1kmd1f1Lhxbaec4
```

This is the view that matters for execution. Archer's liquidity is spread across
independent maker books, and a taker's swap builds a virtual unified book across
all of them. `liquidity` reconstructs that view: price levels in priority order
with the size resting at each, aggregated across every maker quoting there.

**Vault balances and total value locked:**

```bash
archer observe vaults --market u8tnfCb1JSSghuNFquQ2beStYgAN1kmd1f1Lhxbaec4
archer observe tvl    --market u8tnfCb1JSSghuNFquQ2beStYgAN1kmd1f1Lhxbaec4
```

**A single maker's book,** including its levels, locked and free balances on
both sides, and how stale its quotes are:

```bash
archer observe maker-book --maker-book <MAKER_BOOK_PDA>
```

**Which makers are actually funded** on a market — useful before routing size,
since an unfunded book is skipped during matching:

```bash
archer maker list-funded --market u8tnfCb1JSSghuNFquQ2beStYgAN1kmd1f1Lhxbaec4
```

**The registry** of maker books admitted to a market:

```bash
archer registry show --market u8tnfCb1JSSghuNFquQ2beStYgAN1kmd1f1Lhxbaec4
```

**Delegated trading accounts,** by address, or by owner and platform:

```bash
archer observe archer-account --account <ARCHER_ACCOUNT_PDA>
archer observe archer-account --owner <WALLET> --platform <PLATFORM>
```

---

## Creating a market

Market creation is permissionless — no allowlist, no approval. Anyone can stand
up a market for any token pair.

```bash
archer market init \
  --base-mint  So11111111111111111111111111111111111111112 \
  --quote-mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v \
  --base-token-program  TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA \
  --quote-token-program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA \
  --base-decimals 9 --quote-decimals 6 \
  --base-lot-size 1000000 --quote-lot-size 1 \
  --tick-size 1000 \
  --raw-base-units-per-base-unit 1 \
  --price 148.50
```

Fees are set by the protocol at a fixed rate for every permissionlessly created
market, so there are no fee flags — the command fills them in and shows you what
they'll be.

**`--price` is the flag worth using.** It changes nothing on chain; it turns the
market's raw integer parameters into numbers you can sanity-check, and it's how
you catch a bad configuration before paying for it:

```
  Maker fee PPM                400 PPM (fixed by protocol)
  Taker fee PPM                700 PPM (fixed by protocol)

═══ Fee Analysis ═══
  Taker fee:                   7.00 bps
  Min trade for 1 atom taker fee: $0.001429
  Price resolution:            6,734 ticks at $148.50
```

The command validates in two passes before it will submit. First an economic
analysis — tick resolution at your reference price, whether lot and tick sizes
divide cleanly, whether the fee split is solvent — reported with specific
remediation advice. Then the program's own rules, applied locally through the
SDK, so an invalid market fails at your terminal rather than as a paid
transaction.

Add `--confirm` to skip the interactive prompt, and `--dry-run` to simulate
without sending.

---

## Running a market you created

Creating a market makes you its **admin**, which is a revenue position: you
collect the market's share of the fees it generates.

Archer splits collected fees 80/20 — 80% to the market admin, 20% to the
protocol treasury — and `fees collect` performs that split in one instruction,
sending each side to the right place.

```bash
archer fees collect \
  --market u8tnfCb1JSSghuNFquQ2beStYgAN1kmd1f1Lhxbaec4 \
  --amount 1000000
```

Check what's available to collect first with `observe market`, which reports
`uncollected_fees_quote_lots`.

The admin position is transferable, so a market's economics can move to a
treasury, a multisig, or a buyer:

```bash
archer admin transfer \
  --market u8tnfCb1JSSghuNFquQ2beStYgAN1kmd1f1Lhxbaec4 \
  --new-admin <NEW_ADMIN>
```

This is irreversible and takes effect immediately.

---

## Global flags

Available on every command:

| Flag | Purpose |
|---|---|
| `-u, --url` | RPC endpoint. Defaults to your Solana CLI config. |
| `-k, --keypair` | Signing keypair. Defaults to your Solana CLI config. |
| `-c, --commitment` | `confirmed` (default) or `finalized`. |
| `--output` | `table` (default) or `json`, for piping into other tools. |
| `--dry-run` | Simulate without sending. |
| `--priority-fee` | Micro-lamports per compute unit. |

`--output json` makes every read command scriptable:

```bash
archer market list --output json | jq -r '.[] | select(.taker_fee_ppm == 0) | .market'
```

---

## Building on Archer

This CLI is a thin layer over [`archer-sdk`](https://github.com/ballista-tech/archer-sdk),
which exposes the same functionality as a Rust library — account layouts,
instruction builders, quoting math, limit orders, and an async RPC client. If
you're writing a bot, an integration, or a frontend backend, use the SDK
directly.